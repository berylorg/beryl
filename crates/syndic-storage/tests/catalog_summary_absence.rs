#![cfg(feature = "test-faults")]

mod catalog_source_support;
mod support;

use beryl_home_store::{CommandCancellation, CommandOutcome, HomeCommand};
use beryl_model::ProjectionRevision;
use syndic_storage::{
    ThreadCatalogSummaryPreparation,
    test_faults::{FixtureBatch, FixtureRecord},
};

use catalog_source_support::Fixture;
use support::id;

#[test]
fn absent_derived_summary_is_prepared_with_package_initial_revision_and_published_exactly() {
    let fixture = Fixture::new("absent-summary-publication");
    fixture.create(3);
    fixture.remove_summary(3);
    let prepared = fixture.replacement(3);
    assert_eq!(prepared.replacement().thread_id(), id(3));
    assert_eq!(
        prepared.replacement().revision(),
        ProjectionRevision::new(1).unwrap()
    );
    let expected = prepared.replacement().clone();
    fixture.execute(fixture.storage.rebuild_thread_catalog_summary(prepared));
    assert_eq!(fixture.summary(3), expected);
    assert!(matches!(
        fixture
            .storage
            .prepare_thread_catalog_summary(&fixture.store, id(3))
            .unwrap()
            .unwrap(),
        ThreadCatalogSummaryPreparation::ExactCurrent(_)
    ));
}

#[test]
fn raced_absence_and_canonical_source_change_reject_without_overwriting() {
    let fixture = Fixture::new("absent-summary-races");
    fixture.create(3);
    let initial = fixture.summary(3);
    fixture.remove_summary(3);
    let absent = fixture.replacement(3);
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ThreadCatalogSummary(initial.clone()))
        .unwrap();
    fixture.batch(batch);
    let revision = fixture.store.home_revision().unwrap();
    let mut command = HomeCommand::new(revision);
    command
        .add(fixture.storage.rebuild_thread_catalog_summary(absent))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(fixture.summary(3), initial);
    assert_eq!(fixture.store.home_revision().unwrap(), revision);

    fixture.remove_summary(3);
    let absent = fixture.replacement(3);
    fixture.change_activity(3, 10);
    let revision = fixture.store.home_revision().unwrap();
    let mut command = HomeCommand::new(revision);
    command
        .add(fixture.storage.rebuild_thread_catalog_summary(absent))
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert!(
        fixture
            .storage
            .thread_catalog_summary(
                &fixture.store,
                id(3),
                syndic_storage::SyndicPointReadLimit::new(65_536).unwrap()
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn cancellation_leaves_summary_absent_and_pair_publication_joins_absence_with_replacement() {
    let fixture = Fixture::new("absent-summary-pair");
    fixture.create(3);
    fixture.create(4);
    fixture.remove_summary(3);
    fixture.change_activity(4, 10);
    let absent = fixture.replacement(3);
    let existing = fixture.replacement(4);
    let cancel = CommandCancellation::new();
    cancel.cancel();
    let revision = fixture.store.home_revision().unwrap();
    let mut command = HomeCommand::new(revision).with_cancellation(cancel);
    command
        .add(
            fixture
                .storage
                .rebuild_thread_catalog_summary(absent.clone()),
        )
        .unwrap();
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    let expected_missing = absent.replacement().clone();
    let expected_existing = existing.replacement().clone();
    fixture.execute(
        fixture
            .storage
            .publish_thread_catalog_summary_pair(
                ThreadCatalogSummaryPreparation::PreparedReplacement(absent),
                Some(ThreadCatalogSummaryPreparation::PreparedReplacement(
                    existing,
                )),
            )
            .unwrap(),
    );
    assert_eq!(fixture.summary(3), expected_missing);
    assert_eq!(fixture.summary(4), expected_existing);
}

#[test]
fn missing_canonical_thread_is_not_a_summary_insertion_preparation() {
    let fixture = Fixture::new("absent-summary-canonical-owner");
    assert!(
        fixture
            .storage
            .prepare_thread_catalog_summary(&fixture.store, id(3))
            .unwrap()
            .is_none()
    );
}
