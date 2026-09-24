#![cfg(feature = "test-faults")]
#[path = "generated_input_admission/draft.rs"]
mod draft;
mod support;

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    *,
};
use beryl_model::*;
use syndic_storage::{test_faults::FixtureRecord, *};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}
fn seeded(
    home: &support::TestHome,
    faults: FaultController,
) -> (HomeStore, SyndicStorage, DiscussionParentRequest) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let source = support::generated_input::seed_pending(&store, &storage);
    (store, storage, source)
}
fn request(text: &str) -> GeneratedDiscussionInput {
    GeneratedDiscussionInput {
        parent_turn_id: SyndicTurnId::from_bytes([234; 16]),
        canonical_item_id: SyndicItemId::from_bytes([232; 16]),
        resolution: text.to_owned(),
        admitted_at: support::timestamp(100),
    }
}
fn lookup(
    store: &HomeStore,
    source: DiscussionParentRequest,
    text: &str,
) -> GeneratedDiscussionInputLookup {
    let DiscussionHandoffGateState::Pending {
        intent_id,
        job_id,
        resolving_turn_id,
    } = source.child_gate.state()
    else {
        panic!("pending handoff")
    };
    GeneratedDiscussionInputLookup {
        home_id: store.home_id(),
        parent_thread_id: source.parent_thread_id,
        child_thread_id: source.child_gate.thread_id(),
        intent_id,
        job_id,
        context_owner: source.context_owner,
        context_digest: source.context_digest,
        resolving_turn_id,
        parent_turn_id: request(text).parent_turn_id,
        canonical_item_id: request(text).canonical_item_id,
        resolution: text.to_owned(),
    }
}
fn proven(
    store: &HomeStore,
    storage: &SyndicStorage,
    source: DiscussionParentRequest,
) -> PreparedDiscussionParent {
    let DiscussionParentEligibility::Proven(parent) =
        storage.prepare_discussion_parent(store, source).unwrap()
    else {
        panic!("ready parent")
    };
    parent
}
fn command(store: &HomeStore, prepared: PreparedGeneratedDiscussionInput) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(prepared.into_contribution()).unwrap();
    command
}

#[test]
fn admission_is_unpublished_until_commit_and_preserves_existing_draft_editor() {
    let home = support::TestHome::new("generated-draft-preservation");
    let (store, storage, source) = seeded(&home, FaultController::new());
    let session = draft::edit(&store, &storage, source.parent_thread_id);
    let lookup = lookup(&store, source, "literal [image:A]\ntext");
    let before = storage
        .current_draft(&store, source.parent_thread_id, limit())
        .unwrap()
        .unwrap();
    let request = request("literal [image:A]\ntext");
    let prepared = storage
        .prepare_generated_discussion_input(&store, proven(&store, &storage, source), request)
        .unwrap();
    let intent = prepared.intent();
    assert!(
        storage
            .content_manifest(&store, intent.input().content().id(), limit())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage
            .generated_discussion_input_status(&store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::ExactOld
    );
    assert_eq!(
        storage
            .discover_generated_discussion_input(&store, &lookup)
            .unwrap(),
        GeneratedDiscussionInputDiscovery::Absent
    );
    support::discussion_input::committed(&store, prepared.into_contribution());
    let after = storage
        .current_draft(&store, source.parent_thread_id, limit())
        .unwrap()
        .unwrap();
    assert_eq!(before.draft(), after.draft());
    assert_eq!(
        before.thread().current_draft_id(),
        after.thread().current_draft_id()
    );
    assert_eq!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(session)
    );
    assert_eq!(intent.input().ordinal(), AcceptedInputOrdinal::FIRST);
    assert_eq!(
        storage
            .input_gate(&store, source.parent_thread_id, limit())
            .unwrap()
            .unwrap()
            .live_count(),
        0
    );
    assert_eq!(
        storage
            .generated_discussion_input_status(&store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::ExactNew
    );
    assert_eq!(
        storage
            .discover_generated_discussion_input(&store, &lookup)
            .unwrap(),
        GeneratedDiscussionInputDiscovery::Exact
    );
    assert!(matches!(
        storage.prepare_discussion_parent(&store, source).unwrap(),
        DiscussionParentEligibility::Waiting
    ));
    let execution = support::exact_cas::establish_turn(
        &store,
        storage.clone(),
        source.parent_thread_id,
        SyndicTurnId::from_bytes([234; 16]),
        support::timestamp(101),
    );
    support::exact_cas::admit_event(
        &store,
        storage.clone(),
        source.parent_thread_id,
        SyndicTurnId::from_bytes([234; 16]),
        &execution,
        SourceEventPayload::TurnActivated,
        support::timestamp(101),
    );
    support::exact_cas::correlate_user_item(
        &store,
        storage.clone(),
        source.parent_thread_id,
        SyndicTurnId::from_bytes([234; 16]),
        SyndicItemId::from_bytes([232; 16]),
        &execution,
        support::timestamp(102),
    );
    assert_eq!(
        storage
            .generated_discussion_input_status(&store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::Collision
    );
    assert_eq!(
        storage
            .discover_generated_discussion_input(&store, &lookup)
            .unwrap(),
        GeneratedDiscussionInputDiscovery::Exact
    );
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    drop(intent);
    store.close().unwrap();
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(
        storage
            .discover_generated_discussion_input(&store, &lookup)
            .unwrap(),
        GeneratedDiscussionInputDiscovery::Exact
    );
    let mut wrong = lookup.clone();
    wrong.resolution.push('!');
    assert_eq!(
        storage
            .discover_generated_discussion_input(&store, &wrong)
            .unwrap(),
        GeneratedDiscussionInputDiscovery::Collision
    );
    store.close().unwrap();
}

#[test]
fn invalid_text_timestamp_and_raw_identity_aliases_publish_nothing() {
    let home = support::TestHome::new("generated-rejection");
    let (store, storage, source) = seeded(&home, FaultController::new());
    let original = store.home_revision().unwrap();
    let parent = proven(&store, &storage, source);
    for text in [String::new(), "x".repeat(65_537), "🦀".repeat(65_537)] {
        assert!(
            storage
                .prepare_generated_discussion_input(&store, parent.clone(), request(&text))
                .is_err()
        );
    }
    let mut stale = request("text");
    stale.admitted_at = support::timestamp(1);
    assert!(matches!(
        storage.prepare_generated_discussion_input(&store, parent.clone(), stale),
        Err(SyndicMutationError::TimestampRegressed)
    ));
    for turn in [
        SyndicTurnId::from_bytes([31; 16]),
        SyndicTurnId::from_bytes([32; 16]),
        SyndicTurnId::from_bytes([231; 16]),
    ] {
        let mut collision = request("text");
        collision.parent_turn_id = turn;
        assert!(
            storage
                .prepare_generated_discussion_input(&store, parent.clone(), collision)
                .is_err()
        );
    }
    let mut collision = request("text");
    collision.canonical_item_id = SyndicItemId::from_bytes([33; 16]);
    assert!(
        storage
            .prepare_generated_discussion_input(&store, parent, collision)
            .is_err()
    );
    assert_eq!(store.home_revision().unwrap(), original);
    let gate = storage
        .input_gate(&store, source.parent_thread_id, limit())
        .unwrap()
        .unwrap();
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::InputGate(
            InputGateRecord::new(
                gate.thread_id(),
                gate.revision(),
                gate.state().clone(),
                u64::MAX,
                gate.route_generation_high_water(),
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        )]),
    );
    let overflow_revision = store.home_revision().unwrap();
    assert!(
        storage
            .prepare_generated_discussion_input(
                &store,
                proven(&store, &storage, source),
                request("text")
            )
            .is_err()
    );
    assert_eq!(store.home_revision().unwrap(), overflow_revision);
    store.close().unwrap();
}

#[test]
fn exact_outcomes_survive_recovery_and_old_handles_cannot_admit() {
    for (fault, expected) in [
        (
            FaultPoint::BeforeCommit,
            GeneratedDiscussionInputStatus::ExactOld,
        ),
        (
            FaultPoint::AfterCommitBeforePersist,
            GeneratedDiscussionInputStatus::ExactNew,
        ),
    ] {
        let home = support::TestHome::new("generated-recovery");
        let faults = FaultController::new();
        let (store, storage, source) = seeded(&home, faults.clone());
        let parent = proven(&store, &storage, source);
        let maximum = "🦀".repeat(65_536);
        let lookup = lookup(&store, source, &maximum);
        let prepared = storage
            .prepare_generated_discussion_input(&store, parent.clone(), request(&maximum))
            .unwrap();
        let intent = prepared.intent();
        let command = command(&store, prepared);
        faults.fail_next(fault);
        match store.execute(command) {
            CommandOutcome::NotCommitted { .. } if fault == FaultPoint::BeforeCommit => {}
            CommandOutcome::Indeterminate { reconciliation, .. }
                if fault == FaultPoint::AfterCommitBeforePersist =>
            {
                reconciliation.install();
            }
            other => panic!("unexpected fault outcome: {other:?}"),
        }
        if store.health().state() == HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            storage
                .generated_discussion_input_status_candidate(&access, &intent)
                .is_err()
        );
        assert_eq!(
            fresh
                .generated_discussion_input_status_candidate(&access, &intent)
                .unwrap(),
            expected
        );
        assert_eq!(
            fresh
                .discover_generated_discussion_input_candidate(&access, &lookup)
                .unwrap(),
            if expected == GeneratedDiscussionInputStatus::ExactNew {
                GeneratedDiscussionInputDiscovery::Exact
            } else {
                GeneratedDiscussionInputDiscovery::Absent
            }
        );
        for pending in access.pending_reconciliations() {
            assert!(matches!(
                access.reconcile(&pending).unwrap(),
                ReconciliationResolution::ExactNew { .. }
            ));
        }
        let store = recovery.publish().unwrap();
        assert!(
            fresh
                .prepare_generated_discussion_input(&store, parent, request("text"))
                .is_err()
        );
        assert_eq!(
            fresh
                .generated_discussion_input_status(&store, &intent)
                .unwrap(),
            expected
        );
        store.close().unwrap();
    }
}

#[test]
fn content_owner_extras_and_mixed_publication_are_collisions() {
    let home = support::TestHome::new("generated-content-collision");
    let (store, storage, source) = seeded(&home, FaultController::new());
    let parent = proven(&store, &storage, source);
    let prepared = storage
        .prepare_generated_discussion_input(&store, parent.clone(), request("text"))
        .unwrap();
    let intent = prepared.intent();
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::ContentChunk(
            ContentChunkRecord::new(
                intent.input().content().id(),
                ContentChunkOrdinal::new(99).unwrap(),
                vec![1],
            )
            .unwrap(),
        )]),
    );
    assert_eq!(
        storage
            .generated_discussion_input_status(&store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::Collision
    );
    assert!(
        storage
            .prepare_generated_discussion_input(
                &store,
                proven(&store, &storage, source),
                request("text")
            )
            .is_err()
    );
    assert!(
        storage
            .prepare_generated_discussion_input(&store, parent, request("different text"))
            .is_err()
    );
    let outcome = store.execute(command(&store, prepared));
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
    store.close().unwrap();
}

#[test]
fn exact_content_reuse_retains_its_revision_and_foreign_homes_reject_witnesses() {
    let home = support::TestHome::new("generated-content-reuse");
    let (store, storage, source) = seeded(&home, FaultController::new());
    let lookup = lookup(&store, source, "shared");
    let payload = PreparedContent::composer(
        &ComposerPayload::new(vec![
            ComposerAtom::text("Discussion resolution:\n\nshared").unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    support::stage_prepared_content(&store, storage.clone(), &payload);
    let original = storage
        .content_manifest(&store, payload.id(), limit())
        .unwrap()
        .unwrap();
    let prepared = storage
        .prepare_generated_discussion_input(
            &store,
            proven(&store, &storage, source),
            request("shared"),
        )
        .unwrap();
    let intent = prepared.intent();
    assert_eq!(
        storage
            .generated_discussion_input_status(&store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::ExactOld
    );
    support::discussion_input::committed(&store, prepared.into_contribution());
    assert_eq!(
        storage
            .content_manifest(&store, payload.id(), limit())
            .unwrap()
            .unwrap(),
        original
    );
    assert_eq!(
        storage
            .generated_discussion_input_status(&store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::ExactNew
    );
    let other = support::TestHome::new("generated-foreign");
    let (other_store, other_storage, _) = seeded(&other, FaultController::new());
    assert!(
        other_storage
            .generated_discussion_input_status(&other_store, &intent)
            .is_err()
    );
    assert!(
        other_storage
            .discover_generated_discussion_input(&other_store, &lookup)
            .is_err()
    );
    other_store.close().unwrap();
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
