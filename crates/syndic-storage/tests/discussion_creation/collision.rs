use super::*;
use beryl_model::{SyndicDraftId, ThreadRevision};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};

#[test]
fn each_discussion_owned_key_blocks_creation_and_partial_outcomes() {
    let home = TestHome::new("discussion-owned-collisions");
    let (store, storage) = seeded(&home);
    let prepared = prepare(&store, &storage);
    let intent = prepared.intent().clone();
    drop(prepared);
    let witness = source(&store, &storage);
    let owner = DiscussionContextOwnerId::Draft(intent.draft_id());
    let records = [
        (
            FixtureRecord::DiscussionHandoffGate(DiscussionHandoffGateRecord::open(
                intent.thread_id(),
            )),
            FixtureDelete::DiscussionHandoffGate(intent.thread_id()),
        ),
        (
            FixtureRecord::ContextEnvelope(ContextEnvelopeRecord::new(
                owner,
                ContextEnvelopeRevision::FIRST,
                DiscussionContextEnvelope::new(
                    witness.source(),
                    witness.text().clone(),
                    timestamp(100),
                )
                .unwrap(),
            )),
            FixtureDelete::ContextEnvelope(owner),
        ),
        (
            FixtureRecord::ThreadParent(ThreadParentIndexRecord::new(
                id(30),
                intent.thread_id(),
                ThreadRevision::new(1).unwrap(),
                owner,
            )),
            FixtureDelete::ThreadParent {
                parent: id(30),
                child: intent.thread_id(),
            },
        ),
    ];
    for (record, deletion) in records {
        support::commit(&store, storage.clone(), support::batch([record]));
        assert_eq!(
            storage.discussion_creation_status(&store, &intent).unwrap(),
            ThreadCreationStatus::Collision
        );
        let prepared = prepare(&store, &storage);
        assert!(matches!(
            store.execute(command(&store, prepared)),
            CommandOutcome::NotCommitted { .. }
        ));
        assert!(
            storage
                .thread(&store, intent.thread_id(), limit())
                .unwrap()
                .is_none()
        );
        let mut batch = FixtureBatch::new();
        batch.delete(deletion).unwrap();
        support::commit(&store, storage.clone(), batch);
        assert_eq!(
            storage.discussion_creation_status(&store, &intent).unwrap(),
            ThreadCreationStatus::Absent
        );
    }
    store.close().unwrap();
}

#[test]
fn draft_turn_and_accepted_input_aliases_cannot_be_reused() {
    let home = TestHome::new("discussion-alias-collisions");
    let (store, storage) = seeded(&home);
    for draft in [
        draft_id(31),
        SyndicDraftId::from_bytes(*source_turn().as_bytes()),
        SyndicDraftId::from_bytes(*support::populated::next_input().as_bytes()),
    ] {
        let prepared = storage
            .prepare_discussion_creation(
                &store,
                source(&store, &storage),
                CreateDiscussion::new(
                    id(210),
                    draft,
                    timestamp(100),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            )
            .unwrap();
        let intent = prepared.intent().clone();
        assert!(matches!(
            store.execute(command(&store, prepared)),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(
            storage.discussion_creation_status(&store, &intent).unwrap(),
            ThreadCreationStatus::Collision
        );
        assert!(
            storage
                .thread(&store, intent.thread_id(), limit())
                .unwrap()
                .is_none()
        );
    }
    store.close().unwrap();
}
