#![cfg(feature = "test-faults")]

mod support;

#[path = "phase58_accepted_next_pages/support.rs"]
mod accepted_next_support;
#[path = "phase53_accepted_delivery/fixtures.rs"]
mod delivery_fixture;
#[path = "phase58_accepted_promotion/support.rs"]
mod promotion_support;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    AcceptedInputRevision, DraftRevision, InputGateRevision, SyndicAcceptedInputId, SyndicItemId,
    SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use promotion_support::{Fixture, promotion_fixture};
use support::{
    TestHome, batch, commit, draft_id, empty_composer_content, id, open,
    seed_detached_canonical_draft_backing, timestamp,
};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn seeded(name: &str, fixture: Fixture) -> (TestHome, HomeStore, SyndicStorage, Fixture) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let _ = seed_detached_canonical_draft_backing(
        &store,
        storage.clone(),
        beryl_model::SyndicThreadId::from_bytes([0xf1; 16]),
        fixture.current_draft,
    );
    commit(&store, storage.clone(), batch(fixture.records.clone()));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    (home, store, storage, fixture)
}

fn promotion(store: &HomeStore, storage: &SyndicStorage) -> PromoteAcceptedInput {
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = storage
        .accepted_next_source_page(store, storage.revision(store).unwrap(), None, limits)
        .unwrap();
    let candidate = storage
        .accepted_next_candidate_page(store, sources.records()[0], None, limits)
        .unwrap()
        .into_candidate()
        .unwrap();
    PromoteAcceptedInput::new(
        candidate,
        SyndicTurnId::from_bytes([170; 16]),
        SyndicItemId::from_bytes([171; 16]),
        timestamp(20),
    )
}

fn route_leaf(
    store: &HomeStore,
    storage: &SyndicStorage,
    input: SyndicAcceptedInputId,
) -> AcceptedRouteLeafRecord {
    let route = storage
        .input_gate(store, id(40), limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap();
    storage
        .accepted_route_page(store, id(40), route.generation(), route.revision(), None)
        .unwrap()
        .records()
        .iter()
        .find(|entry| entry.input().id() == input)
        .unwrap()
        .leaf()
        .clone()
}

fn scrub_and_reopen_rejects_missing_transition_witness(
    home: &TestHome,
    store: HomeStore,
    storage: SyndicStorage,
    input: SyndicAcceptedInputId,
) {
    let leaf = route_leaf(&store, &storage, input);
    assert!(leaf.last_transition().is_some());
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedRouteLeaf(
            AcceptedRouteLeafRecord::new(
                leaf.input_id(),
                leaf.thread_id(),
                leaf.generation(),
                leaf.ordinal(),
                leaf.revision(),
                leaf.state(),
                leaf.lifecycle(),
            ),
        )]),
    );
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("transitioned accepted-route leaf is missing its witness")
    );
    store.close().unwrap();
    let mut reopened = open(home.path());
    let _storage = SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("transitioned accepted-route leaf is missing its witness")
    );
    reopened.close().unwrap();
}

#[test]
fn durable_delivery_transition_witnesses_are_required_by_scrub_and_reopen() {
    for (name, input) in [
        ("delivering", delivery_fixture::delivering_input()),
        ("retryable", delivery_fixture::retryable_input()),
    ] {
        let home = TestHome::new(&format!("phase57-transition-witness-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        delivery_fixture::seed_mixed_abandonment(&store, storage.clone());
        scrub_and_reopen_rejects_missing_transition_witness(&home, store, storage, input);
    }
}

#[test]
fn exact_projection_loss_transition_witness_is_required_by_scrub_and_reopen() {
    let home = TestHome::new("phase57-projection-loss-witness");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    delivery_fixture::seed_mixed_abandonment(&store, storage.clone());
    let generic = delivery_fixture::abandonment_request(&store, &storage);
    let request = AbandonActiveBinding::after_exact_rejection(
        generic.thread_id(),
        generic.expected_binding_revision(),
        generic.route_generation(),
        generic.target().clone(),
        generic.selected_path(),
        generic.stale().clone(),
        ExactRejectedInputDelivery::new(
            delivery_fixture::delivering_input(),
            AcceptedInputRevision::new(2).unwrap(),
        ),
    );
    assert!(matches!(
        store.execute_current(storage.current_abandon_active_binding(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    scrub_and_reopen_rejects_missing_transition_witness(
        &home,
        store,
        storage,
        delivery_fixture::delivering_input(),
    );
}

#[test]
fn durable_promotion_witness_is_required_by_explicit_scrub_and_survives_routine_reopen() {
    let fixture = promotion_fixture(57, id(57));
    let (home, store, storage, fixture) = seeded("phase57-promotion-witness", fixture);
    let request = promotion(&store, &storage);
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.promote_accepted_input(request))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let generation = AcceptedRouteGeneration::FIRST;
    let route = syndic_storage::test_faults::accepted_route_generation(
        &store,
        storage.clone(),
        fixture.thread,
        generation,
    )
    .unwrap();
    let leaf = storage
        .accepted_route_page(&store, fixture.thread, generation, route.revision(), None)
        .unwrap()
        .records()
        .iter()
        .find(|entry| entry.input().id() == fixture.accepted_input)
        .unwrap()
        .leaf()
        .clone();
    assert!(leaf.promotion().is_some());
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedRouteLeaf(
            AcceptedRouteLeafRecord::new(
                leaf.input_id(),
                leaf.thread_id(),
                leaf.generation(),
                leaf.ordinal(),
                leaf.revision(),
                leaf.state(),
                leaf.lifecycle(),
            ),
        )]),
    );
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("promoted accepted-route leaf is missing its successor witness")
    );
    store.close().unwrap();
    let mut reopened = open(home.path());
    let _storage = SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("promoted accepted-route leaf is missing its successor witness")
    );
    reopened.close().unwrap();
}

#[test]
fn projection_loss_source_keeps_next_work_ineligible_while_the_gate_is_not_idle() {
    let home = TestHome::new("phase57-projection-loss");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    delivery_fixture::seed_mixed_abandonment(&store, storage.clone());
    let request = delivery_fixture::abandonment_request(&store, &storage);
    assert!(matches!(
        store.execute_current(storage.current_abandon_active_binding(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let source = storage
        .accepted_next_source_page(
            &store,
            storage.revision(&store).unwrap(),
            None,
            CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap(),
        )
        .unwrap()
        .records()[0];
    let page = storage
        .accepted_next_candidate_page(
            &store,
            source,
            None,
            CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap(),
        )
        .unwrap();
    assert!(page.into_candidate().is_none());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn direct_accepted_identity_chain_corruption_is_rejected() {
    let source = draft_id(97);
    assert_eq!(
        AcceptedInputAdmissionProof::new(
            ThreadRevision::new(1).unwrap(),
            source,
            DraftRevision::new(1).unwrap(),
            InputGateRevision::new(1).unwrap(),
            source,
        ),
        Err(SyndicRecordError::AcceptedInputAdmissionDraftCollision)
    );
    assert!(matches!(
        AcceptedInputRecord::new(
            SyndicAcceptedInputId::from_bytes([99; 16]),
            id(57),
            AcceptedInputOrdinal::FIRST,
            AcceptedInputAdmissionProof::new(
                ThreadRevision::new(1).unwrap(),
                source,
                DraftRevision::new(1).unwrap(),
                InputGateRevision::new(1).unwrap(),
                draft_id(98),
            )
            .unwrap(),
            AcceptedRouteGeneration::FIRST,
            empty_composer_content(),
            None,
            timestamp(1),
        ),
        Err(SyndicRecordError::AcceptedInputIdentityMismatch)
    ));
    let fixture = promotion_fixture(59, id(59));
    let (_home, store, storage, fixture) = seeded("phase57-identity-chain", fixture);
    let input = storage
        .accepted_input(&store, fixture.accepted_input, limit())
        .unwrap()
        .unwrap();
    let proof = input.admission();
    let corrupt = AcceptedInputRecord::new(
        input.id(),
        input.thread_id(),
        input.ordinal(),
        AcceptedInputAdmissionProof::new(
            proof.expected_thread_revision(),
            proof.source_draft_id(),
            proof.expected_draft_revision(),
            proof.expected_gate_revision(),
            draft_id(99),
        )
        .unwrap(),
        input.route_generation(),
        input.content(),
        input.asset_reference_set(),
        input.admitted_at(),
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::AcceptedInput(corrupt)]),
    );
    let error = store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("accepted-input replacement descendant is not exclusive")
    );
}
