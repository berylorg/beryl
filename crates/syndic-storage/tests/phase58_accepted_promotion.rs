#![cfg(feature = "test-faults")]

mod support;

#[path = "phase58_accepted_next_pages/support.rs"]
mod accepted_next_support;
#[path = "support/activity_handoff.rs"]
mod activity_handoff;
#[path = "phase58_accepted_promotion/descendants.rs"]
mod descendants;
#[path = "phase58_accepted_promotion/newer_generation.rs"]
mod newer_generation;
#[path = "phase58_accepted_promotion/prior_delivery_witness.rs"]
mod prior_delivery_witness;
#[path = "phase58_accepted_promotion/support.rs"]
mod promotion_support;

use beryl_home_store::{CommandError, CommandOutcome, CursorReadLimits, HomeCommand};
use beryl_model::{SyndicItemId, SyndicPathDigest, SyndicTurnId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use promotion_support::{Fixture, promotion_fixture};
use support::{TestHome, batch, commit, id, open, timestamp};

fn seeded(
    name: &str,
) -> (
    TestHome,
    beryl_home_store::HomeStore,
    SyndicStorage,
    Fixture,
) {
    seeded_fixture(name, promotion_fixture(90, id(90)))
}

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
    commit(&store, storage, batch(fixture.records.clone()));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    (home, store, storage, fixture)
}

fn candidate(store: &beryl_home_store::HomeStore, storage: SyndicStorage) -> AcceptedNextCandidate {
    let revision = storage.revision(store).unwrap();
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = storage
        .accepted_next_source_page(store, revision, None, limits)
        .unwrap();
    assert_eq!(sources.records().len(), 1);
    storage
        .accepted_next_candidate_page(store, sources.records()[0], None, limits)
        .unwrap()
        .into_candidate()
        .expect("fixture owns effective next-turn input")
}

fn promotion(store: &beryl_home_store::HomeStore, storage: SyndicStorage) -> PromoteAcceptedInput {
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

fn execute(
    store: &beryl_home_store::HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected command to commit without later failure, got {outcome:?}"),
    }
}

fn assert_transcript_advance_conflict(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    build: &TranscriptBuildRecord,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.advance_transcript_build(
            storage.revision(store).unwrap(),
            AdvanceTranscriptBuild::new(build.thread_id(), build.generation(), build.revision()),
        ))
        .unwrap();
    let error = match store.execute(command) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected definitive transcript conflict, got {outcome:?}"),
    };
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected transcript mutation validation rejection");
    };
    assert!(matches!(
        source.downcast_ref::<SyndicMutationError>(),
        Some(SyndicMutationError::TranscriptBuildConflict)
    ));
}

fn start_transcript_build(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
) -> (TranscriptViewHeadRecord, TranscriptBuildRecord) {
    let thread = storage.thread(store, thread_id, limit()).unwrap().unwrap();
    let stale = storage
        .transcript_view_head(store, thread_id, limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread_id, thread.revision(), stale.revision()),
        ),
    );
    let head = storage
        .transcript_view_head(store, thread_id, limit())
        .unwrap()
        .unwrap();
    let build = storage
        .transcript_build(store, thread_id, head.generation(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.revision(), build.revision());
    (head, build)
}

fn finish_transcript_build(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    generation: TranscriptGeneration,
) -> TranscriptBuildRecord {
    for _ in 0..64 {
        let build = storage
            .transcript_build(store, thread_id, generation, limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return build;
        }
        assert_ne!(build.phase(), TranscriptBuildPhase::Superseded);
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread_id, generation, build.revision()),
            ),
        );
    }
    panic!("fixture transcript build did not finish within its bounded path");
}

fn admit_pending_input(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    fixture: &Fixture,
    next_draft: u8,
    admitted_at: SyndicTimestamp,
) {
    let current = storage
        .current_draft(store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.admit_accepted_input(
            storage.revision(store).unwrap(),
            AcceptedInputAdmission::new(
                fixture.thread,
                current.thread().revision(),
                current.draft().id(),
                current.draft().revision(),
                current.draft().content(),
                gate.revision(),
                beryl_model::SyndicDraftId::from_bytes([next_draft; 16]),
                None,
                admitted_at,
            ),
        ),
    );
}

#[test]
fn promotion_creates_one_exact_pending_turn_and_preserves_the_current_draft() {
    let (home, store, storage, fixture) = seeded("phase58-promote-exact");
    let promotion = promotion(&store, storage);
    let draft_before = storage
        .draft(&store, fixture.current_draft, limit())
        .unwrap()
        .unwrap();
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
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected promotion to commit without later failure, got {outcome:?}"),
    }

    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    let draft_after = storage
        .draft(&store, fixture.current_draft, limit())
        .unwrap()
        .unwrap();
    assert_eq!(draft_after, draft_before);
    let thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(thread.current_draft_id(), fixture.current_draft);
    assert_eq!(thread.committed_tail(), Some(promotion.successor_turn_id()));
    let gate = storage
        .input_gate(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::PendingTurn(promotion.successor_turn_id())
    );
    assert_eq!(gate.live_next_turn_count(), 0);
    let item = storage
        .canonical_item(&store, promotion.successor_item_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(item.presentation_content(), Some(fixture.accepted_content));
    let accepted = storage
        .accepted_input(&store, fixture.accepted_input, limit())
        .unwrap()
        .unwrap();
    assert_eq!(accepted.content(), fixture.accepted_content);
    assert!(
        storage
            .accepted_next_source_page(
                &store,
                storage.revision(&store).unwrap(),
                None,
                CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap(),
            )
            .unwrap()
            .records()
            .is_empty()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    drop(store);
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn queued_admission_preserves_and_finishes_an_active_transcript_build() {
    let (home, store, storage, fixture) = seeded("phase63-admission-preserves-transcript-build");
    let promotion = promotion(&store, storage);
    execute(&store, storage.promote_accepted_input(promotion.clone()));
    let (active_head, active_build) = start_transcript_build(&store, storage, fixture.thread);
    assert!(matches!(
        active_build.phase(),
        TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. }
    ));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );

    admit_pending_input(&store, storage, &fixture, 124, timestamp(22));

    let preserved = storage
        .transcript_build(&store, fixture.thread, active_build.generation(), limit())
        .unwrap()
        .unwrap();
    let selected = storage
        .transcript_view_head(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(preserved, active_build);
    assert_eq!(selected, active_head);
    let advanced_thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        advanced_thread.revision(),
        active_build
            .source_thread_revision()
            .checked_next()
            .unwrap()
    );
    let summary = storage
        .history_summary(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(summary.thread_revision(), advanced_thread.revision());
    assert!(!summary.complete());

    let completed =
        finish_transcript_build(&store, storage, fixture.thread, active_build.generation());
    assert_eq!(
        completed.source_thread_revision(),
        active_build.source_thread_revision()
    );
    assert_eq!(completed.phase(), TranscriptBuildPhase::Complete);
    let selected = storage
        .transcript_view_head(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(selected.generation(), completed.generation());
    assert_eq!(selected.revision(), completed.revision());
    assert_eq!(selected.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    drop(store);
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .transcript_view_head(&reopened, fixture.thread, limit())
            .unwrap()
            .unwrap(),
        selected,
    );
    assert_eq!(
        reopened_storage
            .transcript_build(
                &reopened,
                fixture.thread,
                active_build.generation(),
                limit(),
            )
            .unwrap()
            .unwrap(),
        completed,
    );
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn transcript_start_uses_the_observed_thread_revision_as_a_floor() {
    let (_home, store, storage, fixture) = seeded("phase63-transcript-start-revision-floor");
    let promotion = promotion(&store, storage);
    execute(&store, storage.promote_accepted_input(promotion));
    let observed_thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let observed_head = storage
        .transcript_view_head(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();

    admit_pending_input(&store, storage, &fixture, 126, timestamp(22));
    let current_thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert!(current_thread.revision() > observed_thread.revision());

    execute(
        &store,
        storage.start_transcript_build(
            storage.revision(&store).unwrap(),
            StartTranscriptBuild::new(
                fixture.thread,
                observed_thread.revision(),
                observed_head.revision(),
            ),
        ),
    );
    let build = storage
        .transcript_build(&store, fixture.thread, observed_head.generation(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(build.source_thread_revision(), current_thread.revision());
    assert_eq!(build.committed_tail(), current_thread.committed_tail());
    assert_eq!(
        build.selected_path_digest(),
        current_thread.selected_path_digest()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn active_transcript_compatibility_rejects_future_revision_tail_and_digest() {
    let (_home, store, storage, fixture) = seeded("phase63-transcript-identity-rejections");
    execute(
        &store,
        storage.promote_accepted_input(promotion(&store, storage)),
    );
    let (_, active) = start_transcript_build(&store, storage, fixture.thread);
    let thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();

    let variants = [
        TranscriptBuildRecord::new(
            active.thread_id(),
            active.generation(),
            active.revision(),
            thread.revision().checked_next().unwrap(),
            active.committed_tail(),
            active.selected_path_digest(),
            active.path_turn_count(),
            active.entry_count(),
            active.entry_digest(),
            active.history_complete(),
            active.phase(),
        ),
        TranscriptBuildRecord::new(
            active.thread_id(),
            active.generation(),
            active.revision(),
            active.source_thread_revision(),
            Some(SyndicTurnId::from_bytes([222; 16])),
            active.selected_path_digest(),
            active.path_turn_count(),
            active.entry_count(),
            active.entry_digest(),
            active.history_complete(),
            active.phase(),
        ),
        TranscriptBuildRecord::new(
            active.thread_id(),
            active.generation(),
            active.revision(),
            active.source_thread_revision(),
            active.committed_tail(),
            SyndicPathDigest::from_bytes([223; 32]),
            active.path_turn_count(),
            active.entry_count(),
            active.entry_digest(),
            active.history_complete(),
            active.phase(),
        ),
    ];
    for invalid in variants {
        commit(
            &store,
            storage,
            batch([FixtureRecord::TranscriptBuild(invalid)]),
        );
        assert_transcript_advance_conflict(&store, storage, &invalid);
    }
    commit(
        &store,
        storage,
        batch([FixtureRecord::TranscriptBuild(active)]),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn queued_admission_preserves_a_completed_current_transcript() {
    let (home, store, storage, fixture) = seeded("phase63-admission-preserves-current-transcript");
    let promotion = promotion(&store, storage);
    execute(&store, storage.promote_accepted_input(promotion.clone()));
    let (active_head, _) = start_transcript_build(&store, storage, fixture.thread);
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    let complete =
        finish_transcript_build(&store, storage, fixture.thread, active_head.generation());
    let current = storage
        .transcript_view_head(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(complete.phase(), TranscriptBuildPhase::Complete);
    assert_eq!(current.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(current.revision(), complete.revision());
    let summary_before = storage
        .history_summary(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );

    admit_pending_input(&store, storage, &fixture, 125, timestamp(22));

    assert_eq!(
        storage
            .transcript_view_head(&store, fixture.thread, limit())
            .unwrap()
            .unwrap(),
        current,
    );
    assert_eq!(
        storage
            .transcript_build(&store, fixture.thread, active_head.generation(), limit(),)
            .unwrap()
            .unwrap(),
        complete,
    );
    let advanced_thread = storage
        .thread(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let summary_after = storage
        .history_summary(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(summary_after.thread_revision(), advanced_thread.revision());
    assert_eq!(summary_after.complete(), summary_before.complete());
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    drop(store);
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .transcript_view_head(&reopened, fixture.thread, limit())
            .unwrap()
            .unwrap(),
        current,
    );
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn stale_candidate_and_fresh_identity_collision_do_not_partially_promote() {
    let (_home, store, storage, fixture) = seeded("phase58-promote-races");
    let stale = promotion(&store, storage);
    let colliding = PromoteAcceptedInput::new(
        candidate(&store, storage),
        fixture.current_draft.submitted_turn_id(),
        SyndicItemId::from_bytes([122; 16]),
        timestamp(20),
    );
    let mut collision_command = HomeCommand::new(store.home_revision().unwrap());
    collision_command
        .add(storage.promote_accepted_input(colliding.clone()))
        .unwrap();
    match store.execute(collision_command) {
        CommandOutcome::NotCommitted { .. } => {}
        outcome => panic!("expected definitive promotion collision, got {outcome:?}"),
    }
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &colliding, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Collision
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &stale, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );

    let mut exact_command = HomeCommand::new(store.home_revision().unwrap());
    exact_command
        .add(storage.promote_accepted_input(stale.clone()))
        .unwrap();
    match store.execute(exact_command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => {
            panic!("expected exact promotion to commit without later failure, got {outcome:?}")
        }
    }
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &stale, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
}
