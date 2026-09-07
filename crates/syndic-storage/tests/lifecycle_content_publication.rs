#![cfg(feature = "test-faults")]

mod support;

#[path = "lifecycle_content_publication/conflicts.rs"]
mod conflicts;
#[path = "lifecycle_content_publication/support.rs"]
mod content_support;
#[path = "lifecycle_content_publication/faults.rs"]
mod faults;

use beryl_home_store::{CommandOutcome, CursorReadLimits};
use beryl_model::ContentRevision;
use syndic_storage::{
    ContentByteSpanRecord, SyndicPointReadLimit, prepare_lifecycle_continuation_content,
};

use content_support::*;

#[test]
fn atomic_publication_is_exact_sealed_and_reusable_without_revision_advance() {
    let (home, store, storage) = fixture("exact");
    let before = store.home_revision().unwrap();
    assert_committed(
        store.execute_current(storage.current_publish_lifecycle_continuation_content()),
    );
    assert_eq!(
        store.home_revision().unwrap(),
        before.checked_next().unwrap()
    );
    assert_exact(&store, &storage, ContentRevision::new(1).unwrap());
    let records = snapshot(&store, &storage);
    assert_eq!(records.len(), 5);
    let revision = store.home_revision().unwrap();
    let domain_revision = storage.revision(&store).unwrap();
    for _ in 0..3 {
        assert_already_published(
            store.execute_current(storage.current_publish_lifecycle_continuation_content()),
            ContentRevision::new(1).unwrap(),
        );
    }
    assert_eq!(snapshot(&store, &storage), records);
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(storage.revision(&store).unwrap(), domain_revision);
    store.close().unwrap();
    let mut reopened = crate::support::open(home.path());
    let storage = syndic_storage::SyndicStorage::register(&mut reopened).unwrap();
    assert_exact(&reopened, &storage, ContentRevision::new(1).unwrap());
    assert_eq!(snapshot(&reopened, &storage), records);
    reopened.close().unwrap();
}

#[test]
fn concurrent_requests_publish_once_and_reuse_the_same_closure() {
    let (_home, store, storage) = fixture("concurrent");
    let before = store.home_revision().unwrap();
    let barrier = std::sync::Barrier::new(4);
    let outcomes = std::thread::scope(|scope| {
        let workers = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let command = storage.current_publish_lifecycle_continuation_content();
                    barrier.wait();
                    store.execute_current(command)
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut committed = 0;
    for outcome in outcomes {
        if matches!(
            &outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ) {
            committed += 1;
        } else {
            assert_already_published(outcome, ContentRevision::new(1).unwrap());
        }
    }
    assert_eq!(committed, 1);
    assert_eq!(
        store.home_revision().unwrap(),
        before.checked_next().unwrap()
    );
    assert_exact(&store, &storage, ContentRevision::new(1).unwrap());
    store.close().unwrap();
}

#[test]
fn complete_existing_sealed_content_preserves_its_stored_revision() {
    let (_home, store, storage) = fixture("existing-revision");
    let revision = ContentRevision::new(17).unwrap();
    seed(&store, &storage, exact_records(revision));
    let before = snapshot(&store, &storage);
    let home_revision = store.home_revision().unwrap();
    assert_already_published(
        store.execute_current(storage.current_publish_lifecycle_continuation_content()),
        revision,
    );
    assert_eq!(snapshot(&store, &storage), before);
    assert_eq!(store.home_revision().unwrap(), home_revision);
    assert_exact(&store, &storage, revision);
    store.close().unwrap();
}

fn assert_exact(
    store: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    revision: ContentRevision,
) {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let id = prepared.id();
    let manifest = storage
        .content_manifest(store, id, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(manifest, prepared.sealed_manifest(revision));
    let limits = CursorReadLimits::new(8, 65_536).unwrap();
    let chunks = storage.content_chunks(store, id, None, limits).unwrap();
    assert_eq!(chunks.records(), prepared.chunks());
    assert!(!chunks.has_more());
    let spans = storage.content_byte_spans(store, id, None, limits).unwrap();
    assert_eq!(
        spans.records(),
        &[ContentByteSpanRecord::for_chunk(&prepared.chunks()[0], 0).unwrap()]
    );
    assert!(!spans.has_more());
    let text = storage.content_text_spans(store, id, None, limits).unwrap();
    assert_eq!(text.records(), prepared.text_spans());
    assert!(!text.has_more());
    let pieces = storage.content_pieces(store, id, None, limits).unwrap();
    assert_eq!(pieces.records(), prepared.pieces());
    assert!(!pieces.has_more());
}
