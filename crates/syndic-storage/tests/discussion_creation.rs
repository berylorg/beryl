#![cfg(feature = "test-faults")]

mod support;

#[path = "discussion_creation/recovery.rs"]
mod recovery;

#[path = "discussion_creation/inheritance.rs"]
mod inheritance;

#[path = "discussion_creation/gate.rs"]
mod gate;

#[path = "discussion_creation/collision.rs"]
mod collision;

use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{DiscussionContextOwnerId, SyndicItemId};
use support::populated::{source_item, source_projection, source_turn};
use support::{TestHome, draft_id, id, open, seed_populated, timestamp};
use syndic_storage::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn source(store: &HomeStore, storage: &SyndicStorage) -> PreparedDiscussionSource {
    let thread = storage.thread(store, id(30), limit()).unwrap().unwrap();
    let head = storage
        .transcript_view_head(store, id(30), limit())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            id(30),
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.projection_id() == source_projection())
        .unwrap();
    storage
        .prepare_discussion_source(
            store,
            DiscussionContextSource::new(
                id(30),
                source_turn(),
                source_item(),
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, 9).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new("assistant").unwrap(),
        )
        .unwrap()
}

fn prepare(store: &HomeStore, storage: &SyndicStorage) -> PreparedDiscussionCreation {
    storage
        .prepare_discussion_creation(
            store,
            source(store, storage),
            CreateDiscussion::new(
                id(210),
                draft_id(211),
                timestamp(100),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        )
        .unwrap()
}

fn command(store: &HomeStore, prepared: PreparedDiscussionCreation) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    command
}

fn seeded(home: &TestHome) -> (HomeStore, SyndicStorage) {
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    (store, storage)
}

#[test]
fn atomic_discussion_closure_preserves_source_and_first_submission_parent() {
    let home = TestHome::new("discussion-creation");
    let (store, storage) = seeded(&home);
    let parent_before = storage.thread(&store, id(30), limit()).unwrap();
    let parent_draft_before = storage
        .current_draft(&store, id(30), limit())
        .unwrap()
        .unwrap()
        .draft()
        .clone();
    let prepared = prepare(&store, &storage);
    let intent = prepared.intent().clone();
    assert_eq!(
        storage.discussion_creation_status(&store, &intent).unwrap(),
        ThreadCreationStatus::Absent
    );
    assert!(matches!(
        store.execute(command(&store, prepared)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage.discussion_creation_status(&store, &intent).unwrap(),
        ThreadCreationStatus::Exact
    );
    let child = storage
        .thread(&store, intent.thread_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(child.parent_thread_id(), Some(id(30)));
    assert_eq!(child.committed_tail(), Some(source_turn()));
    assert_eq!(child.lineage_depth().get(), 2);
    assert_eq!(child.lineage_ancestor_skip(), Some(id(30)));
    let context = storage
        .context_envelope(
            &store,
            DiscussionContextOwnerId::Draft(intent.draft_id()),
            limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(context.envelope().text().as_str(), "assistant");
    assert_eq!(
        storage
            .discussion_handoff_gate(&store, child.id(), limit())
            .unwrap(),
        Some(DiscussionHandoffGateRecord::open(child.id()))
    );
    assert!(
        storage
            .discussion_handoff_gate(&store, id(30), limit())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        storage.thread(&store, id(30), limit()).unwrap(),
        parent_before
    );
    assert_eq!(
        storage
            .current_draft(&store, id(30), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &parent_draft_before
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let turn = support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        child.id(),
        draft_id(212),
        SyndicItemId::from_bytes([213; 16]),
        "discuss it",
        timestamp(101),
    );
    let submitted = storage.turn(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(submitted.parent().turn(), Some(source_turn()));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn stale_preparation_and_reused_identity_do_not_create_partial_discussions() {
    let home = TestHome::new("discussion-stale");
    let (store, storage) = seeded(&home);
    let stale = prepare(&store, &storage);
    let fresh = prepare(&store, &storage);
    let intent = stale.intent().clone();
    assert!(matches!(
        store.execute(command(&store, fresh)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(matches!(
        store.execute(command(&store, stale)),
        CommandOutcome::NotCommitted { .. }
    ));
    let duplicate = prepare(&store, &storage);
    assert!(matches!(
        store.execute(command(&store, duplicate)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage.discussion_creation_status(&store, &intent).unwrap(),
        ThreadCreationStatus::Exact
    );
    store.close().unwrap();
}
