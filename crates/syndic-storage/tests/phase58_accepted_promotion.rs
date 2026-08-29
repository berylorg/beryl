#![cfg(feature = "test-faults")]

mod support;

#[path = "phase58_accepted_next_pages/support.rs"]
mod accepted_next_support;
#[path = "phase58_accepted_promotion/descendants.rs"]
mod descendants;
#[path = "phase58_accepted_promotion/newer_generation.rs"]
mod newer_generation;
#[path = "phase58_accepted_promotion/prior_delivery_witness.rs"]
mod prior_delivery_witness;
#[path = "phase58_accepted_promotion/support.rs"]
mod promotion_support;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand};
use beryl_model::{SyndicItemId, SyndicTurnId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use promotion_support::{Fixture, promotion_fixture, seed_detached_draft_backing};
use support::{TestHome, batch, commit, id, open, timestamp};

fn seeded_fixture(
    name: &str,
    fixture: Fixture,
) -> (
    TestHome,
    beryl_home_store::HomeStore,
    SyndicStorage,
    Fixture,
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let _ = seed_detached_draft_backing(
        &store,
        storage.clone(),
        beryl_model::SyndicThreadId::from_bytes([0xf0; 16]),
        fixture.current_draft,
    );
    commit(&store, storage.clone(), batch(fixture.records.clone()));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    (home, store, storage, fixture)
}

fn candidate(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
) -> AcceptedNextCandidate {
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = storage
        .accepted_next_source_page(store, storage.revision(store).unwrap(), None, limits)
        .unwrap();
    storage
        .accepted_next_candidate_page(
            store,
            *sources
                .records()
                .first()
                .expect("fixture owns at least one next-turn source"),
            None,
            limits,
        )
        .unwrap()
        .into_candidate()
        .expect("fixture owns one effective next-turn input")
}

fn promotion(store: &beryl_home_store::HomeStore, storage: &SyndicStorage) -> PromoteAcceptedInput {
    PromoteAcceptedInput::new(
        candidate(store, storage),
        SyndicTurnId::from_bytes([120; 16]),
        SyndicItemId::from_bytes([121; 16]),
        timestamp(20),
    )
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

fn execute_promotion(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    promotion: PromoteAcceptedInput,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.promote_accepted_input(promotion))
        .unwrap();
    store.execute(command)
}

#[test]
fn promotion_creates_one_exact_pending_turn_and_preserves_the_current_draft() {
    let (home, store, storage, fixture) =
        seeded_fixture("phase58-promote-exact", promotion_fixture(90, id(90)));
    let request = promotion(&store, &storage);
    let draft_before = storage
        .draft(&store, fixture.current_draft, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );
    assert!(matches!(
        execute_promotion(&store, &storage, request.clone()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    assert_eq!(
        storage
            .draft(&store, fixture.current_draft, limit())
            .unwrap()
            .unwrap(),
        draft_before
    );
    let thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(thread.committed_tail(), Some(request.successor_turn_id()));
    assert_eq!(thread.current_draft_id(), fixture.current_draft);
    let parent = fixture
        .records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::Thread(thread) if thread.id() == fixture.thread => {
                thread.committed_tail()
            }
            _ => None,
        })
        .unwrap();
    let successor = storage
        .turn(&store, request.successor_turn_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(successor.parent(), ConversationParent::Turn(parent));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &request, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn stale_candidate_and_fresh_identity_collision_do_not_partially_promote() {
    let (_home, store, storage, _fixture) =
        seeded_fixture("phase58-promote-races", promotion_fixture(91, id(91)));
    let stale = promotion(&store, &storage);
    let colliding = PromoteAcceptedInput::new(
        candidate(&store, &storage),
        stale.successor_turn_id(),
        SyndicItemId::from_bytes([122; 16]),
        timestamp(20),
    );
    assert!(matches!(
        execute_promotion(&store, &storage, colliding.clone()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &colliding, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    assert!(matches!(
        execute_promotion(&store, &storage, stale.clone()),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &stale, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Collision
    );
}
