#![cfg(feature = "test-faults")]

#[path = "phase53_named_rejection_abandonment/generic_witness_corruption.rs"]
mod generic_witness_corruption;
mod support;

use beryl_home_store::{
    CommandError, CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{AcceptedInputRevision, CasTurnId, InputGateRevision};
use syndic_storage::test_faults::{FixtureRecord, fixture_route_leaf_with_transition};
use syndic_storage::*;

use support::phase11::{
    DELIVERY_UNKNOWN_LOGICAL_BYTES, abandonment_request, delivering_input, retryable_input,
    seed_mixed_abandonment,
};
use support::populated::steering_input;
use support::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn named_request(store: &HomeStore, storage: SyndicStorage) -> AbandonActiveBinding {
    let generic = abandonment_request(store, storage);
    AbandonActiveBinding::after_exact_rejection(
        generic.thread_id(),
        generic.expected_binding_revision(),
        generic.route_generation(),
        generic.target().clone(),
        generic.selected_path(),
        generic.stale().clone(),
        ExactRejectedInputDelivery::new(delivering_input(), AcceptedInputRevision::new(2).unwrap()),
    )
}

fn seed(store: &HomeStore, storage: SyndicStorage) -> AbandonActiveBinding {
    seed_mixed_abandonment(store, storage);
    let route = syndic_storage::test_faults::accepted_route_generation(
        store,
        storage,
        id(40),
        AcceptedRouteGeneration::FIRST,
    )
    .unwrap();
    let page = storage
        .accepted_route_page(store, id(40), route.generation(), route.revision(), None)
        .unwrap();
    let leaf = page
        .records()
        .iter()
        .find(|entry| entry.input().id() == retryable_input())
        .unwrap()
        .leaf()
        .clone();
    commit(
        store,
        storage,
        batch([
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    route.thread_id(),
                    route.generation(),
                    route.revision(),
                    route.target().clone(),
                    route.first_ordinal(),
                    route.last_ordinal(),
                    route.input_count(),
                    1,
                    2,
                    route.next_turn_count(),
                    route.terminal_count(),
                    route.live_logical_utf8_bytes(),
                    route.delivering_logical_utf8_bytes(),
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedRouteLeaf(fixture_route_leaf_with_transition(
                AcceptedRouteLeafRecord::new(
                    leaf.input_id(),
                    leaf.thread_id(),
                    leaf.generation(),
                    leaf.ordinal(),
                    leaf.revision().checked_next().unwrap(),
                    leaf.state(),
                    AcceptedInputLifecycle::Delivering,
                ),
                AcceptedRouteLeafTransitionProof::new(
                    InputGateRevision::new(5).unwrap(),
                    AcceptedRouteHeadProof::new(
                        leaf.generation(),
                        AcceptedRouteRevision::new(2).unwrap(),
                    ),
                    leaf.revision(),
                    AcceptedRouteLeafTransitionKind::Begin,
                ),
            )),
        ]),
    );
    named_request(store, storage)
}

fn generic_request(request: &AbandonActiveBinding) -> AbandonActiveBinding {
    AbandonActiveBinding::new(
        request.thread_id(),
        request.expected_binding_revision(),
        request.route_generation(),
        request.target().clone(),
        request.selected_path(),
        request.stale().clone(),
    )
}

fn route_page(store: &HomeStore, storage: SyndicStorage) -> (InputGateRecord, AcceptedRoutePage) {
    let gate = storage.input_gate(store, id(40), limit()).unwrap().unwrap();
    let proof = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(store, id(40), proof.generation(), proof.revision(), None)
        .unwrap();
    (gate, page)
}

fn row<'a>(
    page: &'a AcceptedRoutePage,
    input: beryl_model::SyndicAcceptedInputId,
) -> &'a AcceptedRouteEntry {
    page.records()
        .iter()
        .find(|row| row.input().id() == input)
        .unwrap()
}

#[test]
fn exact_unverdictable_rejection_is_preserved_while_sibling_delivery_becomes_unknown() {
    let home = TestHome::new("phase53-named-rejection-abandonment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let request = seed(&store, storage);
    let generic = generic_request(&request);

    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    assert!(matches!(
        store.execute_current(storage.current_abandon_active_binding(request.clone())),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &request, limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &generic, limit())
            .unwrap(),
        BindingPublicationStatus::Collision,
        "a named abandonment must not authenticate the generic command",
    );

    let (gate, page) = route_page(&store, storage);
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 3);
    assert_eq!(
        gate.live_logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );

    let named = row(&page, delivering_input());
    assert_eq!(
        named.effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(named.leaf().lifecycle(), AcceptedInputLifecycle::Retryable);
    assert_eq!(
        named.leaf().revision(),
        AcceptedInputRevision::new(3).unwrap()
    );
    assert_eq!(
        named.leaf().last_transition(),
        Some(AcceptedRouteLeafTransitionProof::new(
            InputGateRevision::new(6).unwrap(),
            AcceptedRouteHeadProof::new(
                request.route_generation(),
                AcceptedRouteRevision::new(3).unwrap(),
            ),
            AcceptedInputRevision::new(2).unwrap(),
            AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection,
        ))
    );

    let sibling = row(&page, retryable_input());
    assert_eq!(
        sibling.effective_state(),
        AcceptedRouteEffectiveState::DeliveryUnknown
    );
    assert_eq!(
        sibling.leaf().lifecycle(),
        AcceptedInputLifecycle::Delivering
    );
    assert_eq!(
        sibling.leaf().revision(),
        AcceptedInputRevision::new(3).unwrap()
    );

    assert_eq!(
        row(&page, steering_input()).effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    let proof = gate.selected_route().unwrap();
    assert_eq!(proof.revision(), AcceptedRouteRevision::new(4).unwrap());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &request, limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &generic, limit())
            .unwrap(),
        BindingPublicationStatus::Collision
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn named_rejection_reconciliation_rejects_leaf_and_route_drift() {
    let home = TestHome::new("phase53-named-rejection-collision");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let request = seed(&store, storage);

    let wrong_leaf = AbandonActiveBinding::after_exact_rejection(
        request.thread_id(),
        request.expected_binding_revision(),
        request.route_generation(),
        request.target().clone(),
        request.selected_path(),
        request.stale().clone(),
        ExactRejectedInputDelivery::new(delivering_input(), AcceptedInputRevision::new(3).unwrap()),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &wrong_leaf, limit())
            .unwrap(),
        BindingPublicationStatus::Collision
    );

    let wrong_generation = AbandonActiveBinding::after_exact_rejection(
        request.thread_id(),
        request.expected_binding_revision(),
        request.route_generation().checked_next().unwrap(),
        request.target().clone(),
        request.selected_path(),
        request.stale().clone(),
        request.exact_rejected_delivery().unwrap(),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &wrong_generation, limit())
            .unwrap(),
        BindingPublicationStatus::Collision
    );
    store.close().unwrap();
}

#[test]
fn generic_abandonment_witness_rejects_named_reconciliation_and_authority_drift() {
    let home = TestHome::new("phase53-generic-abandonment-witness");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let named = seed(&store, storage);
    let generic = generic_request(&named);

    let AcceptedRouteLostTarget::Steering(target) = generic.target() else {
        panic!("steerable fixture must carry a steering loss target");
    };
    let wrong_target = AbandonActiveBinding::new(
        generic.thread_id(),
        generic.expected_binding_revision(),
        generic.route_generation(),
        AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
            target.pending().clone(),
            CasTurnId::new("wrong-abandonment-target").unwrap(),
        )),
        generic.selected_path(),
        generic.stale().clone(),
    );
    let wrong_generation = AbandonActiveBinding::new(
        generic.thread_id(),
        generic.expected_binding_revision(),
        generic.route_generation().checked_next().unwrap(),
        generic.target().clone(),
        generic.selected_path(),
        generic.stale().clone(),
    );
    for drifted in [&wrong_target, &wrong_generation] {
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&store, drifted, limit())
                .unwrap(),
            BindingPublicationStatus::Collision
        );
        assert!(matches!(
            store.execute_current(storage.current_abandon_active_binding(drifted.clone())),
            CommandOutcome::NotCommitted { .. }
        ));
    }

    assert!(matches!(
        store.execute_current(storage.current_abandon_active_binding(generic.clone())),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &generic, limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &named, limit())
            .unwrap(),
        BindingPublicationStatus::Collision,
        "a generic abandonment must not authenticate the named command",
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &generic, limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &named, limit())
            .unwrap(),
        BindingPublicationStatus::Collision
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn named_rejection_abandonment_fault_cuts_reconcile_old_or_exact() {
    for (name, point, expected) in [
        (
            "phase53-named-abandon-before-commit",
            FaultPoint::BeforeCommit,
            BindingPublicationStatus::Prior,
        ),
        (
            "phase53-named-abandon-after-commit",
            FaultPoint::AfterCommitBeforePersist,
            BindingPublicationStatus::Exact,
        ),
        (
            "phase53-named-abandon-after-persist",
            FaultPoint::AfterPersist,
            BindingPublicationStatus::Exact,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        let request = seed(&store, storage);

        faults.fail_next(point);
        let outcome =
            store.execute_current(storage.current_abandon_active_binding(request.clone()));
        let retained_custody = matches!(&outcome, CommandOutcome::Indeterminate { .. });
        match (point, outcome) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { evidence }) => {
                assert!(matches!(evidence, CommandError::Commit { .. }));
            }
            (
                FaultPoint::AfterCommitBeforePersist,
                outcome @ CommandOutcome::Indeterminate { .. },
            ) => {
                assert!(
                    matches!(
                        &outcome,
                        CommandOutcome::Indeterminate {
                            failure: CommandError::Persistence { .. },
                            ..
                        }
                    ),
                    "unexpected indeterminate outcome at {point:?}: {outcome:?}",
                );
            }
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    receipt,
                    later_failure: Some(CommandError::Persistence { .. }),
                },
            ) => {
                assert_eq!(receipt.home_revision(), store.home_revision().unwrap());
            }
            (_, outcome) => panic!("unexpected exact command outcome at {point:?}: {outcome:?}"),
        }
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&store, &request, limit())
                .unwrap(),
            expected
        );
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        if retained_custody {
            let close_error = store.close().unwrap_err();
            assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
            drop(close_error);
            assert!(
                HomeStore::open(HomeOpenOptions::new(
                    home.path(),
                    HomeSchemaVersion::CURRENT
                ))
                .is_err()
            );
            continue;
        }
        store.close().unwrap();

        let mut reopened = open(home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&reopened, &request, limit())
                .unwrap(),
            expected
        );
        reopened
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        reopened.close().unwrap();
    }
}

#[test]
fn generic_abandonment_fault_cuts_reconcile_old_or_exact() {
    for (name, point, expected) in [
        (
            "phase53-generic-abandon-before-commit",
            FaultPoint::BeforeCommit,
            BindingPublicationStatus::Prior,
        ),
        (
            "phase53-generic-abandon-after-commit",
            FaultPoint::AfterCommitBeforePersist,
            BindingPublicationStatus::Exact,
        ),
        (
            "phase53-generic-abandon-after-persist",
            FaultPoint::AfterPersist,
            BindingPublicationStatus::Exact,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        seed_mixed_abandonment(&store, storage);
        let request = abandonment_request(&store, storage);

        faults.fail_next(point);
        let outcome =
            store.execute_current(storage.current_abandon_active_binding(request.clone()));
        let retained_custody = matches!(&outcome, CommandOutcome::Indeterminate { .. });
        match (point, outcome) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { evidence }) => {
                assert!(matches!(evidence, CommandError::Commit { .. }));
            }
            (FaultPoint::AfterCommitBeforePersist, CommandOutcome::Indeterminate { .. }) => {}
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => {}
            (_, outcome) => panic!("unexpected generic abandonment outcome: {outcome:?}"),
        }
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&store, &request, limit())
                .unwrap(),
            expected
        );
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        if retained_custody {
            let close_error = store.close().unwrap_err();
            assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
            drop(close_error);
            assert!(
                HomeStore::open(HomeOpenOptions::new(
                    home.path(),
                    HomeSchemaVersion::CURRENT
                ))
                .is_err()
            );
            continue;
        }
        store.close().unwrap();

        let mut reopened = open(home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(
            storage
                .abandoned_active_binding_publication_status(&reopened, &request, limit())
                .unwrap(),
            expected
        );
        reopened
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        reopened.close().unwrap();
    }
}
