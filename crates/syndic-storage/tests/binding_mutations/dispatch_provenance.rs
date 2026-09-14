use super::*;
#[path = "dispatch_provenance/codec.rs"]
mod codec;
#[path = "dispatch_provenance/publication.rs"]
mod publication;
use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, ReconciliationResolution, WholeHomeScrubTrigger,
    test_faults::{FaultController, FaultPoint},
};

fn state(store: &HomeStore, storage: &SyndicStorage, turn: SyndicTurnId) -> TurnStateRecord {
    storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap()
}

fn activate_submitted_pending(
    store: &HomeStore,
    storage: &SyndicStorage,
) -> (ActiveFixture, beryl_model::SyndicItemId) {
    let thread = id(170);
    create_thread(store, storage, thread, draft_id(171));
    let item = beryl_model::SyndicItemId::from_bytes([173; 16]);
    let turn = exact_cas::submit_current_draft(
        store,
        storage.clone(),
        thread,
        draft_id(172),
        item,
        "preserve exact submitted input",
        timestamp(4),
    );
    let current = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = SelectedPathProof::new(
        current.committed_tail(),
        current.revision(),
        current.selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("submitted-dispatch-provenance").unwrap();
    let valid = valid_request(store, storage, thread, selected, cas_thread.clone());
    execute(
        store,
        storage.publish_valid_binding(storage.revision(store).unwrap(), valid.clone()),
    );
    let snapshot = SyndicExecutionSnapshotId::from_bytes([174; 16]);
    let activation = ActivateBinding::new(
        thread,
        current_binding_revision(store, storage, thread),
        current_gate_revision(store, storage, thread),
        state(store, storage, turn).revision(),
        selected,
        snapshot,
        turn,
        loaded_generation(31, 171),
        timestamp(5),
    );
    execute(
        store,
        storage.activate_binding(storage.revision(store).unwrap(), activation.clone()),
    );
    (
        ActiveFixture {
            thread,
            turn,
            selected,
            cas_thread,
            cas_turn: beryl_model::CasTurnId::new("unpublished-dispatch-turn").unwrap(),
            snapshot,
            valid,
            activation,
        },
        item,
    )
}

fn cancellation(
    store: &HomeStore,
    storage: &SyndicStorage,
    fixture: &ActiveFixture,
) -> CancelBindingActivation {
    CancelBindingActivation::new(
        fixture.thread,
        current_binding_revision(store, storage, fixture.thread),
        current_gate_revision(store, storage, fixture.thread),
        state(store, storage, fixture.turn).revision(),
        fixture.selected,
        fixture.snapshot,
        fixture.turn,
    )
}

fn retry(
    store: &HomeStore,
    storage: &SyndicStorage,
    fixture: &ActiveFixture,
    snapshot: SyndicExecutionSnapshotId,
) -> ActivateBinding {
    ActivateBinding::new(
        fixture.thread,
        current_binding_revision(store, storage, fixture.thread),
        current_gate_revision(store, storage, fixture.thread),
        state(store, storage, fixture.turn).revision(),
        fixture.selected,
        snapshot,
        fixture.turn,
        loaded_generation(31, 200),
        timestamp(10),
    )
}

fn replace_provenance(
    store: &HomeStore,
    storage: &SyndicStorage,
    turn: SyndicTurnId,
    provenance: TurnDispatchProvenance,
) {
    let prior = state(store, storage, turn);
    let replacement = TurnStateRecord::with_capture_frontiers_and_issue(
        turn,
        prior.revision(),
        prior.lifecycle(),
        prior.source_event_count(),
        prior.item_count(),
        prior.finalized_item_count(),
        prior.open_item_count(),
        prior.history_blocking_item_count(),
        prior.provider_observation_issue(),
        prior.end_status(),
        prior.updated_at(),
        provenance,
    )
    .unwrap();
    commit(
        store,
        storage.clone(),
        batch([FixtureRecord::TurnState(replacement)]),
    );
}

#[test]
fn repeated_cancellations_preserve_turn_and_content_and_advance_exact_provenance() {
    let home = TestHome::new("repeated-dispatch-cancellation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (mut fixture, item) = activate_submitted_pending(&store, &storage);
    let original_input = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    let original = storage
        .turn(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let initial_revision = state(&store, &storage, fixture.turn).revision();
    for byte in [190, 191, 192] {
        let prior = state(&store, &storage, fixture.turn);
        let anchor = TurnDispatchAnchor::new(
            fixture.snapshot,
            current_binding_revision(&store, &storage, fixture.thread),
        );
        assert_eq!(
            prior.dispatch_provenance(),
            TurnDispatchProvenance::Activated(anchor)
        );
        let request = cancellation(&store, &storage, &fixture);
        execute(
            &store,
            storage.cancel_binding_activation(storage.revision(&store).unwrap(), request),
        );
        let cancelled = state(&store, &storage, fixture.turn);
        assert_eq!(
            cancelled.dispatch_provenance(),
            TurnDispatchProvenance::Cancelled(anchor)
        );
        assert_eq!(
            cancelled.revision(),
            prior.revision().checked_next().unwrap()
        );
        assert_eq!(cancelled.source_event_count(), 0);
        assert_eq!(cancelled.lifecycle(), TurnLifecycle::Pending);
        assert_eq!(
            storage
                .canonical_item(&store, item, point_limit())
                .unwrap()
                .unwrap(),
            original_input
        );
        assert_eq!(
            storage
                .turn(&store, fixture.turn, point_limit())
                .unwrap()
                .unwrap(),
            original
        );
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        let snapshot = SyndicExecutionSnapshotId::from_bytes([byte; 16]);
        let activation = retry(&store, &storage, &fixture, snapshot);
        assert_eq!(
            storage
                .binding_activation_status(&store, &activation, point_limit())
                .unwrap(),
            BindingPublicationStatus::Prior
        );
        execute(
            &store,
            storage.activate_binding(storage.revision(&store).unwrap(), activation.clone()),
        );
        assert_eq!(
            storage
                .binding_activation_status(&store, &activation, point_limit())
                .unwrap(),
            BindingPublicationStatus::Exact
        );
        fixture.snapshot = snapshot;
    }
    let final_state = state(&store, &storage, fixture.turn);
    assert_eq!(final_state.revision().get(), initial_revision.get() + 6);
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(state(&reopened, &storage, fixture.turn), final_state);
    assert_eq!(
        storage
            .canonical_item(&reopened, item, point_limit())
            .unwrap()
            .unwrap(),
        original_input
    );
    assert_eq!(
        storage
            .turn(&reopened, fixture.turn, point_limit())
            .unwrap()
            .unwrap(),
        original
    );
    reopened.close().unwrap();
}

#[test]
fn dispatch_state_rejects_erasure_foreign_anchors_and_false_cancellation() {
    for case in 0..5 {
        let home = TestHome::new("dispatch-provenance-corruption");
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let fixture = activate_pending(&store, &storage, 170, false);
        let foreign = activate_pending(&store, &storage, 180, false);
        let anchor = TurnDispatchAnchor::new(
            fixture.snapshot,
            current_binding_revision(&store, &storage, fixture.thread),
        );
        let bad = match case {
            0 => TurnDispatchProvenance::Unattempted,
            1 => TurnDispatchProvenance::ProviderOperation,
            2 => TurnDispatchProvenance::Activated(TurnDispatchAnchor::new(
                foreign.snapshot,
                anchor.binding_revision(),
            )),
            3 => TurnDispatchProvenance::Cancelled(anchor),
            _ => TurnDispatchProvenance::Activated(TurnDispatchAnchor::new(
                fixture.snapshot,
                BindingRevision::new(1).unwrap(),
            )),
        };
        replace_provenance(&store, &storage, fixture.turn, bad);
        assert!(
            store
                .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
                .is_err(),
            "case {case}"
        );
        drop(store);
    }
}

#[test]
fn abandoned_activation_without_provider_identity_cannot_become_safe_by_rebinding() {
    let home = TestHome::new("unidentified-dispatch-rebinding");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, &storage, 170, false);
    let original = state(&store, &storage, fixture.turn);
    let request = abandonment(&store, &storage, fixture.thread, false);
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), request),
    );
    let valid = valid_request(
        &store,
        &storage,
        fixture.thread,
        fixture.selected,
        CasThreadId::new("rebound-undetermined").unwrap(),
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );
    let activation = retry(
        &store,
        &storage,
        &fixture,
        SyndicExecutionSnapshotId::from_bytes([190; 16]),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Collision
    );
    let outcome = execute_outcome(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::TurnLifecycleConflict
    ));
    assert_eq!(state(&store, &storage, fixture.turn), original);
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    replace_provenance(
        &store,
        &storage,
        fixture.turn,
        TurnDispatchProvenance::Cancelled(TurnDispatchAnchor::new(
            fixture.snapshot,
            fixture
                .activation
                .expected_binding_revision()
                .checked_next()
                .unwrap(),
        )),
    );
    assert!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    drop(store);
}

#[test]
fn cancelled_anchor_substitution_cannot_authorize_another_attempt() {
    let home = TestHome::new("cancelled-anchor-substitution");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, &storage, 170, false);
    let foreign = activate_pending(&store, &storage, 180, false);
    for candidate in [&fixture, &foreign] {
        execute(
            &store,
            storage.cancel_binding_activation(
                storage.revision(&store).unwrap(),
                cancellation(&store, &storage, candidate),
            ),
        );
    }
    let forged = state(&store, &storage, foreign.turn).dispatch_provenance();
    replace_provenance(&store, &storage, fixture.turn, forged);
    let before = state(&store, &storage, fixture.turn);
    let activation = retry(
        &store,
        &storage,
        &fixture,
        SyndicExecutionSnapshotId::from_bytes([190; 16]),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Collision,
    );
    let outcome = execute_outcome(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingStateConflict
    ));
    assert_eq!(state(&store, &storage, fixture.turn), before);
    assert!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
    drop(store);
}
