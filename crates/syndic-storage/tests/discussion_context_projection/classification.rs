use super::*;
use beryl_model::{
    CasNativeTurnCount, RecoveryItemSequenceDigest, SyndicExecutionSnapshotId, SyndicTurnId,
};

#[test]
fn ambiguous_unattempted_prefix_requires_package_empty_classification_for_publication_and_activation()
 {
    let (_home, store, storage) = seeded("context-empty-classification");
    native_fixtures::seed_child_at_tail(&store, &storage, id(30), id(230), draft_id(231));
    let pending = native_fixtures::append_pending(
        &store,
        &storage,
        id(230),
        SyndicTurnId::from_bytes([232; 16]),
        SyndicTurnId::from_bytes([29; 16]),
    );
    let request = request_for(&store, &storage, pending.thread);
    let basis = storage
        .prepare_native_projection(&store, &request, point_limit())
        .unwrap()
        .basis();
    assert!(basis.context().is_none());
    let native =
        CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap();
    let recovered = CasLineageProof::recovered(
        RecoveredInjectionProof::new(
            RecoveryProjectionVersion::V1,
            basis.represented_prefix(),
            RecoveryItemSequenceDigest::from_bytes([17; 32]),
            RecoveryItemCount::new(1).unwrap(),
            RecoveryUtf8ByteCount::new(15).unwrap(),
            timestamp(30),
            generation(),
        )
        .unwrap(),
    );
    for lineage in [native, recovered] {
        let raw = PublishValidBinding::new(
            pending.thread,
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution().clone(),
            CasThreadId::new("empty-classified-target").unwrap(),
            basis.represented_prefix(),
            CasNativeTurnCount::ZERO,
            request.tool_profile(),
            lineage,
        );
        assert!(matches!(
            store.execute_current(storage.current_publish_valid_binding(raw)),
            CommandOutcome::NotCommitted { .. }
        ));
    }
    let publish = PublishValidBinding::new(
        pending.thread,
        basis.expected_binding_revision(),
        basis.selected_path(),
        request.execution().clone(),
        CasThreadId::new("empty-classified-target").unwrap(),
        basis.represented_prefix(),
        CasNativeTurnCount::ZERO,
        basis.tool_profile(),
        native,
    );
    let original_wire = test_faults::encode_lineage_fixture(publish.lineage());
    let publish = storage
        .prepare_context_free_publication_cancellable(&store, publish, &|| false)
        .unwrap();
    assert_eq!(
        test_faults::encode_lineage_fixture(publish.lineage()),
        original_wire
    );
    assert!(matches!(
        store.execute_current(storage.current_publish_valid_binding(publish)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let current = storage
        .current_binding(&store, pending.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("published empty projection");
    };
    let gate = storage
        .input_gate(&store, pending.thread, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(&store, pending.pending, point_limit())
        .unwrap()
        .unwrap();
    let activation = ActivateBinding::new(
        pending.thread,
        current.binding().revision(),
        gate.revision(),
        state.revision(),
        basis.selected_path(),
        SyndicExecutionSnapshotId::from_bytes([233; 16]),
        pending.pending,
        generation(),
        timestamp(40),
    );
    assert!(matches!(
        store.execute_current(storage.current_activate_binding(activation.clone())),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage.prepare_context_binding_cancellable(&store, pending.thread, usable, &|| true),
        Err(NativeProjectionError::Context(
            RecoveryProjectionError::Cancelled
        ))
    ));
    let classified = storage
        .prepare_context_binding_cancellable(&store, pending.thread, usable, &|| false)
        .unwrap();
    let before = storage.revision(&store).unwrap();
    support::enroll_fixture_activity(&store, &storage, pending.thread, pending.pending);
    assert!(storage.revision(&store).unwrap() > before);
    assert!(matches!(
        store.execute_current(
            storage.current_activate_binding(activation.with_context_classification(classified))
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    store.close().unwrap();
}

#[test]
fn missing_or_substituted_original_activation_snapshot_never_classifies_as_empty() {
    for missing in [true, false] {
        let (_home, store, storage) = seeded(if missing {
            "context-missing-anchor"
        } else {
            "context-substituted-anchor"
        });
        native_fixtures::seed_child_at_tail(&store, &storage, id(30), id(234), draft_id(235));
        let pending = native_fixtures::append_pending(
            &store,
            &storage,
            id(234),
            SyndicTurnId::from_bytes([236; 16]),
            support::populated::source_turn(),
        );
        let request = request_for(&store, &storage, pending.thread);
        let basis = storage
            .prepare_native_projection(&store, &request, point_limit())
            .unwrap()
            .basis();
        let snapshot_id = SyndicExecutionSnapshotId::from_bytes([35; 16]);
        let mut batch = test_faults::FixtureBatch::new();
        if missing {
            batch
                .delete(test_faults::FixtureDelete::ExecutionSnapshot(snapshot_id))
                .unwrap();
        } else {
            let snapshot = storage
                .execution_snapshot(&store, snapshot_id, point_limit())
                .unwrap()
                .unwrap();
            let wrong = ExecutionSnapshotRecord::new(
                snapshot_id,
                snapshot.thread_id(),
                snapshot.binding_revision(),
                snapshot.activation_gate_revision(),
                snapshot.active_turn_id(),
                CasThreadId::new("substituted-cas-thread").unwrap(),
                snapshot.selected_path(),
                snapshot.represented_base_prefix(),
                snapshot.represented_base_native_turn_count(),
                snapshot.tool_profile(),
                snapshot.lineage(),
                snapshot.execution().clone(),
                snapshot.loaded_generation(),
                snapshot.started_at(),
            );
            batch
                .put(test_faults::FixtureRecord::ExecutionSnapshot(wrong))
                .unwrap();
        }
        support::commit(&store, storage.clone(), batch);
        let raw = PublishValidBinding::new(
            pending.thread,
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution().clone(),
            CasThreadId::new("uncorrelated-target").unwrap(),
            basis.represented_prefix(),
            CasNativeTurnCount::ZERO,
            request.tool_profile(),
            CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap(),
        );
        assert!(matches!(
            store.execute_current(storage.current_publish_valid_binding(raw)),
            CommandOutcome::NotCommitted { .. }
        ));
        assert!(
            storage
                .prepare_native_projection(&store, &request, point_limit())
                .is_err()
        );
        store.close().unwrap();
    }
}

#[test]
fn empty_publication_preparation_refuses_context_tag_two_and_original_cancellation() {
    let (_home, store, storage) = seeded("context-empty-publication-refusals");
    let (basis, seed, native_count) = native_seed(&store, &storage);
    let request = request(&store, &storage);
    let make = |lineage| {
        PublishValidBinding::new(
            id(210),
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution().clone(),
            CasThreadId::new("empty-cannot-erase-context").unwrap(),
            basis.represented_prefix(),
            native_count,
            basis.tool_profile(),
            lineage,
        )
    };
    let ordinary =
        make(CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap());
    assert!(matches!(
        storage.prepare_context_free_publication_cancellable(&store, ordinary.clone(), &|| true),
        Err(NativeProjectionError::Context(
            RecoveryProjectionError::Cancelled
        ))
    ));
    assert!(matches!(
        storage.prepare_context_free_publication_cancellable(&store, ordinary, &|| false),
        Err(NativeProjectionError::DiscussionContextProjectionRequired)
    ));
    let tag_two = make(
        seed.completed_lineage(timestamp(100), generation())
            .unwrap(),
    );
    assert!(matches!(
        storage.prepare_context_free_publication_cancellable(&store, tag_two, &|| false),
        Err(NativeProjectionError::DiscussionContextProjectionRequired)
    ));
    store.close().unwrap();
}

#[test]
fn prepared_empty_publication_rechecks_binding_gate_and_original_source_closure() {
    for drift in [0, 1, 2] {
        let (_home, store, storage) = seeded(&format!("context-empty-publication-drift-{drift}"));
        native_fixtures::seed_child_at_tail(&store, &storage, id(30), id(230), draft_id(231));
        let parent = SyndicTurnId::from_bytes([29; 16]);
        let pending = native_fixtures::append_pending(
            &store,
            &storage,
            id(230),
            SyndicTurnId::from_bytes([232; 16]),
            parent,
        );
        let request = request_for(&store, &storage, pending.thread);
        let basis = storage
            .prepare_native_projection(&store, &request, point_limit())
            .unwrap()
            .basis();
        let raw = PublishValidBinding::new(
            pending.thread,
            basis.expected_binding_revision(),
            basis.selected_path(),
            request.execution().clone(),
            CasThreadId::new("prepared-empty-drift-target").unwrap(),
            basis.represented_prefix(),
            CasNativeTurnCount::ZERO,
            request.tool_profile(),
            CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap(),
        );
        let prepared = storage
            .prepare_context_free_publication_cancellable(&store, raw, &|| false)
            .unwrap();
        match drift {
            0 => {
                let command = PublishUnboundBinding::new(
                    pending.thread,
                    basis.expected_binding_revision(),
                    basis.selected_path(),
                    "binding changed after preparation",
                )
                .unwrap();
                let mut home_command =
                    beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
                home_command
                    .add(
                        storage.publish_unbound_binding(storage.revision(&store).unwrap(), command),
                    )
                    .unwrap();
                execution::committed(store.execute(home_command));
            }
            1 => {
                let gate = storage
                    .input_gate(&store, pending.thread, point_limit())
                    .unwrap()
                    .unwrap();
                let changed = InputGateRecord::new(
                    pending.thread,
                    gate.revision().checked_next().unwrap(),
                    gate.state().clone(),
                    gate.accepted_high_water(),
                    gate.route_generation_high_water(),
                    gate.selected_route(),
                    gate.live_steering_count(),
                    gate.live_next_turn_count(),
                    gate.live_logical_utf8_bytes(),
                )
                .unwrap();
                support::commit(
                    &store,
                    storage.clone(),
                    support::batch([test_faults::FixtureRecord::InputGate(changed)]),
                );
            }
            2 => {
                let state = storage
                    .turn_state(&store, parent, point_limit())
                    .unwrap()
                    .unwrap();
                let changed = TurnStateRecord::new(
                    parent,
                    state.revision().checked_next().unwrap(),
                    state.lifecycle(),
                    state.source_event_count(),
                    state.item_count(),
                    Some(TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap()),
                    state.updated_at(),
                    TurnDispatchProvenance::Activated(TurnDispatchAnchor::new(
                        SyndicExecutionSnapshotId::from_bytes([250; 16]),
                        beryl_model::BindingRevision::new(999).unwrap(),
                    )),
                )
                .unwrap();
                support::commit(
                    &store,
                    storage.clone(),
                    support::batch([test_faults::FixtureRecord::TurnState(changed)]),
                );
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            store.execute_current(storage.current_publish_valid_binding(prepared)),
            CommandOutcome::NotCommitted { .. }
        ));
        store.close().unwrap();
    }
}
