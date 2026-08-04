use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{SyndicItemId, SyndicTurnId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use super::{limit, seeded_fixture};
use crate::{
    promotion_support::{NewerGenerationFixture, promotion_fixture_with_newer_generation},
    support::{id, open, timestamp},
};

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap()
}

fn earliest_candidate(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: &super::Fixture,
    newer_head: AcceptedRouteGenerationHeadRecord,
) -> AcceptedNextCandidate {
    let sources = storage
        .accepted_next_source_page(store, storage.revision(store).unwrap(), None, limits())
        .unwrap();
    assert_eq!(sources.records().len(), 2);
    assert_eq!(
        sources.records()[0].generation(),
        AcceptedRouteGeneration::FIRST
    );
    assert_eq!(
        sources.records()[1].generation(),
        newer_head.proof().generation()
    );
    let candidate = storage
        .accepted_next_candidate_page(store, sources.records()[0], None, limits())
        .unwrap()
        .into_candidate()
        .expect("generation one owns the earliest effective next-turn input");
    assert_eq!(candidate.input_id(), fixture.accepted_input);
    assert_eq!(candidate.ordinal(), AcceptedInputOrdinal::FIRST);
    candidate
}

fn assert_newer_authority(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    newer_head: AcceptedRouteGenerationHeadRecord,
    newer_accepted_input: beryl_model::SyndicAcceptedInputId,
) {
    let sources = storage
        .accepted_next_source_page(store, storage.revision(store).unwrap(), None, limits())
        .unwrap();
    assert_eq!(sources.records().len(), 1);
    let source = sources.records()[0];
    assert_eq!(source.thread_id(), thread);
    assert_eq!(source.generation(), newer_head.proof().generation());
    let page = storage
        .accepted_next_candidate_page(store, source, None, limits())
        .unwrap();
    assert!(
        page.candidate().is_none(),
        "the pending successor turn gates the still-available generation-two candidate"
    );
    let input = storage
        .accepted_input(store, newer_accepted_input, limit())
        .unwrap()
        .expect("generation-two accepted input remains available");
    assert_eq!(input.thread_id(), thread);
    assert_eq!(input.route_generation(), newer_head.proof().generation());
}

fn assert_exact_with_preserved_head(
    store: &HomeStore,
    storage: SyndicStorage,
    promotion: &PromoteAcceptedInput,
) {
    // Exact reconciliation includes the complete candidate-captured route-head record. Its
    // canonical codec makes that record equality byte-exact.
    assert_eq!(
        storage
            .accepted_input_promotion_status(store, promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
}

#[test]
fn promoting_generation_one_preserves_a_newer_same_thread_route_head() {
    let thread = id(92);
    let NewerGenerationFixture {
        fixture,
        newer_generation,
        newer_head,
        newer_accepted_input,
    } = promotion_fixture_with_newer_generation(92, thread);
    assert_eq!(newer_head.thread_id(), thread);
    assert_eq!(newer_head.proof().generation(), newer_generation);
    assert!(fixture.records.iter().any(|record| {
        matches!(
            record,
            FixtureRecord::AcceptedRouteGenerationHead(head) if *head == newer_head
        )
    }));
    let (home, store, storage, fixture) =
        seeded_fixture("phase58-promote-before-newer-head", fixture);
    let candidate = earliest_candidate(&store, storage, &fixture, newer_head);
    let promotion = PromoteAcceptedInput::new(
        candidate,
        SyndicTurnId::from_bytes([123; 16]),
        SyndicItemId::from_bytes([124; 16]),
        timestamp(20),
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.promote_accepted_input(promotion.clone()))
        .unwrap();
    store.execute(command).unwrap();

    assert_exact_with_preserved_head(&store, storage, &promotion);
    assert_newer_authority(&store, storage, thread, newer_head, newer_accepted_input);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_newer_authority(
        &reopened,
        reopened_storage,
        thread,
        newer_head,
        newer_accepted_input,
    );
    assert_exact_with_preserved_head(&reopened, reopened_storage, &promotion);
    reopened.validate_registered_domains().unwrap();
}
