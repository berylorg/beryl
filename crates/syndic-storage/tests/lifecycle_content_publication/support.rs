use beryl_home_store::{CommandError, CommandOutcome, HomeStore};
use beryl_model::ContentRevision;
use syndic_storage::{
    SyndicMutationError, SyndicStorage, prepare_lifecycle_continuation_content,
    test_faults::{FixtureRecord, lifecycle_content_canonical_records},
};

pub fn fixture(name: &str) -> (crate::support::TestHome, HomeStore, SyndicStorage) {
    let home = crate::support::TestHome::new(&format!("lifecycle-content-{name}"));
    let mut store = crate::support::open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    (home, store, storage)
}

pub fn assert_committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "expected clean atomic publication, got {outcome:?}"
    );
}

pub fn assert_already_published(outcome: CommandOutcome, revision: ContentRevision) {
    let error = rejected_error(outcome);
    let SyndicMutationError::LifecycleContentAlreadyPublished { content } = typed_error(&error)
    else {
        panic!("expected exact already-published classification, got {error:?}");
    };
    assert_eq!(
        *content,
        prepare_lifecycle_continuation_content()
            .unwrap()
            .reference(revision)
    );
}

pub fn rejected_error(outcome: CommandOutcome) -> CommandError {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => evidence,
        other => panic!("expected noncommit, got {other:?}"),
    }
}

pub fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected typed validation outcome, got {error:?}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

pub fn snapshot(
    store: &HomeStore,
    storage: &SyndicStorage,
) -> Vec<(&'static str, Vec<u8>, Vec<u8>)> {
    lifecycle_content_canonical_records(store, storage)
}

pub fn exact_records(revision: ContentRevision) -> Vec<FixtureRecord> {
    let content = prepare_lifecycle_continuation_content().unwrap();
    let (_, mut records) = crate::support::prepared_content_records(&content);
    records[0] = FixtureRecord::ContentManifest(content.sealed_manifest(revision));
    records
}

pub fn seed(store: &HomeStore, storage: &SyndicStorage, records: Vec<FixtureRecord>) {
    crate::support::commit(store, storage.clone(), crate::support::batch(records));
}
