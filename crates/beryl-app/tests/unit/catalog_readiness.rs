use super::*;
use crate::cas_projection::initial_start::InitialStartOwner;
use crate::catalog_projection::{
    ThreadCatalogProjectionPreparation, prepare_thread_catalog_projection,
};
use beryl_model::{ExecutionBinding, SyndicDraftId, SyndicThreadId};
use beryl_state::CatalogPointReadLimit;
use std::time::{Duration, Instant};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, HistorySummaryRecord, SyndicPointReadLimit,
    SyndicTimestamp,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

#[path = "../catalog_projection_support/mod.rs"]
mod support;
use support::{Fixture, execute_contribution, native_path};

#[path = "catalog_readiness/repair_outcomes.rs"]
mod repair_outcomes;

fn coordinator(
    fixture: &Fixture,
) -> (
    CatalogSourceCoordinator,
    CatalogSourceReader,
    InitialStartOwner,
) {
    let start = InitialStartOwner::new();
    let service = CatalogSourceCoordinator::prepare(
        Arc::new(fixture.store.service_reference()),
        fixture.syndic.clone(),
        fixture.state.clone(),
        start.gate(),
    )
    .unwrap();
    let reader = service.reader();
    (service, reader, start)
}

fn ready(reader: &CatalogSourceReader, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if matches!(reader.certified_threads(), Ok(actual) if actual == count) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "source did not certify {count} threads: {:?}",
            reader.certified_threads()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn publish(fixture: &Fixture) {
    let ThreadCatalogProjectionPreparation::Publish(command) = prepare_thread_catalog_projection(
        &fixture.store,
        &fixture.syndic,
        &fixture.state,
        fixture.thread_id,
    )
    .unwrap() else {
        panic!("expected repair")
    };
    assert!(matches!(
        fixture.store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn advance_source(fixture: &Fixture, activity: u64) {
    let old = fixture
        .syndic
        .history_summary(
            &fixture.store,
            fixture.thread_id,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            old.thread_id(),
            old.revision().checked_next().unwrap(),
            old.thread_revision(),
            old.committed_tail(),
            old.selected_path_digest(),
            old.complete(),
            SyndicTimestamp::from_unix_millis(activity),
        )))
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture
            .syndic
            .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
    );
}

#[test]
fn missing_projection_population_is_repaired_and_certified_across_multiple_pages() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    for seed in 20..55 {
        execute_contribution(
            &fixture.store,
            fixture.syndic.create_thread(
                fixture.syndic.revision(&fixture.store).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([seed; 16]),
                    SyndicDraftId::from_bytes([seed; 16]),
                    ExecutionBinding::new(
                        fixture.runtime_id,
                        fixture.root_id,
                        native_path(beryl_model::RuntimeMode::host(), r"C:\Work\Beryl"),
                    ),
                    SyndicTimestamp::from_unix_millis(u64::from(seed)),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        );
    }
    let (mut service, reader, start) = coordinator(&fixture);
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::NotReady)
    ));
    start.release();
    ready(&reader, 36);
    let read = reader
        .retain_source(&fixture.store, &CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        certification::certify(
            &fixture.store,
            &fixture.syndic,
            &fixture.state,
            &read,
            &CommandCancellation::new()
        )
        .unwrap(),
        certification::Certification::Ready { threads: 36 }
    ));
    fixture.store.release_frozen_read(&read).unwrap();
    service.stop_and_join().unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn absent_summary_restart_rebuilds_higher_state_witness_without_relaxing_sources() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    publish(&fixture);
    advance_source(&fixture, 100);
    publish(&fixture);
    let before = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(before.sources().syndic_summary().get(), 2);
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::ThreadCatalogSummary(fixture.thread_id))
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture
            .syndic
            .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
    );
    let (mut service, reader, start) = coordinator(&fixture);
    start.release();
    ready(&reader, 1);
    let after = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(after.sources().syndic_summary().get(), 1);
    assert!(after.revision() > before.revision());
    assert_eq!(after.facts().last_activity_at().get(), 100);
    service.stop_and_join().unwrap();
}

#[test]
fn retained_source_keeps_original_snapshot_across_live_commit_and_coordinator_replacement() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    let (mut service, reader, start) = coordinator(&fixture);
    start.release();
    ready(&reader, 1);
    let old = reader
        .retain_source(&fixture.store, &CommandCancellation::new())
        .unwrap();
    let old_row = fixture
        .state
        .catalog()
        .frozen_row(
            &fixture.store,
            &old,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    advance_source(&fixture, 500);
    service.waker().wake_by_ref();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let current = reader
            .retain_source(&fixture.store, &CommandCancellation::new())
            .unwrap();
        let replaced = current.home_revision() != old.home_revision();
        fixture.store.release_frozen_read(&current).unwrap();
        if replaced {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        fixture
            .state
            .catalog()
            .frozen_row(
                &fixture.store,
                &old,
                fixture.thread_id,
                CatalogPointReadLimit::schema_maximum()
            )
            .unwrap()
            .unwrap(),
        old_row
    );
    service.stop_and_join().unwrap();
    assert!(matches!(
        reader.certified_threads(),
        Err(CatalogSourceReadError::Retired)
    ));
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 1);
    fixture.store.release_frozen_read(&old).unwrap();
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn orphaned_index_is_refused_before_any_missing_projection_repair() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    publish(&fixture);
    let row = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    execute_contribution(
        &fixture.store,
        fixture.state.catalog().remove_copy_for_test(
            fixture.state.catalog().revision(&fixture.store).unwrap(),
            row,
            true,
        ),
    );
    let before = fixture.store.home_revision().unwrap();
    let read = fixture
        .store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        certification::certify(
            &fixture.store,
            &fixture.syndic,
            &fixture.state,
            &read,
            &CommandCancellation::new()
        ),
        Err(CatalogSourceReadError::Catalog(_))
    ));
    fixture.store.release_frozen_read(&read).unwrap();
    assert_eq!(fixture.store.home_revision().unwrap(), before);
}

#[test]
fn unstarted_cancelled_coordinator_never_captures_or_repairs() {
    let fixture = Fixture::new(r"C:\Work\Beryl");
    let before = fixture.store.home_revision().unwrap();
    let (mut service, reader, _start) = coordinator(&fixture);
    service.stop_and_join().unwrap();
    assert_eq!(fixture.store.home_revision().unwrap(), before);
    assert_eq!(fixture.store.retained_frozen_read_count().unwrap(), 0);
    assert!(matches!(
        reader.retain_source(&fixture.store, &CommandCancellation::new()),
        Err(CatalogSourceReadError::Retired)
    ));
}

#[test]
fn empty_canonical_population_certifies_and_cancelled_capture_scan_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate =
        beryl_home_store::HomeOpenCandidate::open(beryl_home_store::HomeOpenOptions::new(
            directory.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let frozen = home
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(matches!(
        certification::certify(&home, &syndic, &state, &frozen, &CommandCancellation::new())
            .unwrap(),
        certification::Certification::Ready { threads: 0 }
    ));
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(matches!(
        certification::certify(&home, &syndic, &state, &frozen, &cancellation),
        Err(CatalogSourceReadError::Cancelled)
    ));
    home.release_frozen_read(&frozen).unwrap();
}

#[test]
fn deleted_and_recreated_live_claim_restarts_scalar_under_joined_source_rebuild() {
    use beryl_model::{WindowBounds, WindowDisplayState, WindowId, WindowPlacement};
    use beryl_state::{
        ActivateRestoringClaim, BeginSessionRestore, CreateClaimedWindow, RememberedTarget,
        RemoveSessionWindow,
    };
    let fixture = Fixture::new(r"C:\Work\Beryl");
    let window = fixture.claim_thread();
    let session = fixture.state.session();
    let original = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    execute_contribution(
        &fixture.store,
        session.begin_restore(
            session.revision(&fixture.store).unwrap(),
            BeginSessionRestore::new(original.header().revision()),
        ),
    );
    let restoring = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    execute_contribution(
        &fixture.store,
        session.activate_restoring_claim(
            session.revision(&fixture.store).unwrap(),
            ActivateRestoringClaim::new(
                restoring.header().revision(),
                window,
                restoring.windows()[0].revision(),
                restoring.windows()[0].selected_thread().unwrap(),
            ),
        ),
    );
    publish(&fixture);
    let before = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(before.sources().claim().unwrap().get(), 3);
    let current = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    execute_contribution(
        &fixture.store,
        session.remove_window(
            session.revision(&fixture.store).unwrap(),
            RemoveSessionWindow::new(
                current.header().revision(),
                window,
                current.windows()[0].revision(),
                current.windows()[0].selected_thread(),
            ),
        ),
    );
    let current = session.minimal_bootstrap(&fixture.store).unwrap().unwrap();
    execute_contribution(
        &fixture.store,
        session.create_claimed_window(
            session.revision(&fixture.store).unwrap(),
            CreateClaimedWindow::new(
                current.header().revision(),
                WindowId::from_bytes([6; 16]),
                RememberedTarget::new(fixture.runtime_id, fixture.root_id),
                fixture.thread_id,
                WindowPlacement::new(
                    WindowBounds::new(0, 0, 900, 700).unwrap(),
                    WindowDisplayState::Normal,
                    None,
                    None,
                ),
            ),
        ),
    );
    let (mut service, reader, start) = coordinator(&fixture);
    start.release();
    ready(&reader, 1);
    let after = fixture
        .state
        .catalog()
        .row(
            &fixture.store,
            fixture.thread_id,
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(after.sources().claim().unwrap().get(), 1);
    assert!(after.revision() > before.revision());
    assert_eq!(
        after.facts().claim().window_id(),
        Some(WindowId::from_bytes([6; 16]))
    );
    service.stop_and_join().unwrap();
}
