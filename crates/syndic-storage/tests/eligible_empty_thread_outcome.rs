#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore, MutationContribution};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
};
use beryl_state::{BerylState, InitializeThreadlessWindow};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, EligibleEmptyThreadCandidate,
    EligibleEmptyThreadOutcome, EligibleEmptyThreadOutcomeAudit, HistorySummaryRecord,
    SyndicPointReadLimit, SyndicStorage, ThreadCatalogSummaryPreparation,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use support::{TestHome, draft_id, id, open, timestamp};

fn open_store(home: &TestHome) -> (HomeStore, SyndicStorage) {
    let mut candidate = open(home.path());
    BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    (store, storage)
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([1; 16]),
        RootId::from_bytes([2; 16]),
        RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, r"C:\Work")
            .unwrap(),
    )
}

fn execute(store: &HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn create(store: &HomeStore, storage: &SyndicStorage, seed: u8) {
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                id(seed),
                draft_id(seed),
                execution(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
}

fn window_mutation(store: &HomeStore) -> MutationContribution {
    let state = BerylState::reacquire(store).unwrap();
    state.session().initialize_threadless(
        state.session().revision(store).unwrap(),
        InitializeThreadlessWindow::new(
            beryl_model::WindowId::from_bytes([7; 16]),
            beryl_model::WindowPlacement::new(
                beryl_model::WindowBounds::new(0, 0, 900, 700).unwrap(),
                beryl_model::WindowDisplayState::Normal,
                None,
                None,
            ),
        ),
    )
}

fn candidate(store: &HomeStore, storage: &SyndicStorage) -> EligibleEmptyThreadCandidate {
    storage
        .inspect_eligible_empty_thread(store, id(3), &execution())
        .unwrap()
        .unwrap()
}

fn summary(
    store: &HomeStore,
    storage: &SyndicStorage,
    seed: u8,
) -> ThreadCatalogSummaryPreparation {
    storage
        .prepare_thread_catalog_summary(store, id(seed))
        .unwrap()
        .unwrap()
}

fn outcome(store: &HomeStore, storage: &SyndicStorage) -> EligibleEmptyThreadOutcome {
    storage
        .prepare_eligible_empty_thread_outcome(
            store,
            candidate(store, storage),
            summary(store, storage, 3),
        )
        .unwrap()
}

fn change_activity(store: &HomeStore, storage: &SyndicStorage, at: u64) {
    let old = storage
        .history_summary(store, id(3), SyndicPointReadLimit::new(65_536).unwrap())
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
            timestamp(at),
        )))
        .unwrap();
    execute(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), batch),
    );
}

#[test]
fn unchanged_summary_audits_exact_and_validates_without_mutating_or_deleting() {
    let home = TestHome::new("eligible-unchanged-outcome");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    let proof = outcome(&store, &storage);
    let EligibleEmptyThreadOutcomeAudit::Exact(fresh) = storage
        .audit_eligible_empty_thread_outcome(&store, &proof)
        .unwrap()
    else {
        panic!("unchanged successor must be exact")
    };
    let revision = storage.revision(&store).unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add_validation(
            storage
                .validate_eligible_empty_thread(&store, fresh)
                .unwrap(),
        )
        .unwrap();
    command.add(window_mutation(&store)).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert!(
        storage
            .thread(&store, id(3), SyndicPointReadLimit::new(65_536).unwrap())
            .unwrap()
            .is_some()
    );
}

#[test]
fn planned_successor_requires_exact_publication_and_retains_original_facts() {
    let home = TestHome::new("eligible-planned-outcome");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    change_activity(&store, &storage, 10);
    let original = candidate(&store, &storage);
    let planned = summary(&store, &storage, 3);
    let proof = storage
        .prepare_eligible_empty_thread_outcome(&store, original.clone(), planned.clone())
        .unwrap();
    assert!(matches!(
        storage
            .audit_eligible_empty_thread(&store, &original)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Exact(_)
    ));
    assert_eq!(
        storage
            .audit_eligible_empty_thread_outcome(&store, &proof)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Conflict
    );
    let ThreadCatalogSummaryPreparation::PreparedReplacement(replacement) = planned else {
        panic!("history change must require compact refresh")
    };
    execute(&store, storage.rebuild_thread_catalog_summary(replacement));
    assert!(matches!(
        storage
            .audit_eligible_empty_thread_outcome(&store, &proof)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Exact(_)
    ));
    assert_eq!(
        storage
            .audit_eligible_empty_thread(&store, &original)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Conflict
    );
    change_activity(&store, &storage, 11);
    assert_eq!(
        storage
            .audit_eligible_empty_thread_outcome(&store, &proof)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Conflict
    );
}

#[test]
fn wrong_thread_or_domain_revision_cannot_form_outcome() {
    let home = TestHome::new("eligible-summary-fences");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    create(&store, &storage, 4);
    assert!(
        storage
            .prepare_eligible_empty_thread_outcome(
                &store,
                candidate(&store, &storage),
                summary(&store, &storage, 4)
            )
            .is_err()
    );
    let old = summary(&store, &storage, 3);
    change_activity(&store, &storage, 10);
    assert!(
        storage
            .prepare_eligible_empty_thread_outcome(&store, candidate(&store, &storage), old)
            .is_err()
    );
}

#[test]
fn unrelated_domain_publication_preserves_exact_facts_and_refreshes_validation_candidate() {
    let home = TestHome::new("eligible-unrelated-change");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    let original = candidate(&store, &storage);
    let proof = outcome(&store, &storage);
    create(&store, &storage, 4);
    assert!(
        storage
            .validate_eligible_empty_thread(&store, original)
            .is_err()
    );
    let EligibleEmptyThreadOutcomeAudit::Exact(fresh) = storage
        .audit_eligible_empty_thread_outcome(&store, &proof)
        .unwrap()
    else {
        panic!("unrelated thread cannot alter original closure")
    };
    assert!(
        storage
            .validate_eligible_empty_thread(&store, fresh)
            .is_ok()
    );
}

#[test]
fn stable_partial_closure_is_conflict_and_absent_thread_is_missing() {
    let home = TestHome::new("eligible-missing-outcome");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    let proof = outcome(&store, &storage);
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::ThreadAttributes(id(3)))
        .unwrap();
    execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), batch),
    );
    assert_eq!(
        storage
            .audit_eligible_empty_thread_outcome(&store, &proof)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Conflict
    );
    let mut batch = FixtureBatch::new();
    batch.delete(FixtureDelete::Thread(id(3))).unwrap();
    execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), batch),
    );
    assert_eq!(
        storage
            .audit_eligible_empty_thread_outcome(&store, &proof)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Missing
    );
}

#[test]
fn original_pure_evidence_survives_same_home_reopen_but_old_candidate_cannot_validate() {
    let home = TestHome::new("eligible-reopened-outcome");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    let original = candidate(&store, &storage);
    let proof = outcome(&store, &storage);
    drop(storage);
    store.close().unwrap();
    let (store, storage) = open_store(&home);
    assert!(
        storage
            .validate_eligible_empty_thread(&store, original.clone())
            .is_err()
    );
    assert!(
        storage
            .reuse_empty_thread_with_catalog_predecessor(
                &store,
                original,
                summary(&store, &storage, 3),
                None,
            )
            .is_err()
    );
    let EligibleEmptyThreadOutcomeAudit::Exact(fresh) = storage
        .audit_eligible_empty_thread_outcome(&store, &proof)
        .unwrap()
    else {
        panic!("same durable home must authenticate original facts")
    };
    assert!(
        storage
            .validate_eligible_empty_thread(&store, fresh)
            .is_ok()
    );
}

#[test]
fn coincident_foreign_home_facts_cannot_authenticate_original_outcome() {
    let first = TestHome::new("eligible-home-one");
    let second = TestHome::new("eligible-home-two");
    let (first_store, first_storage) = open_store(&first);
    let (second_store, second_storage) = open_store(&second);
    create(&first_store, &first_storage, 3);
    create(&second_store, &second_storage, 3);
    let proof = outcome(&first_store, &first_storage);
    assert_eq!(
        second_storage
            .audit_eligible_empty_thread_outcome(&second_store, &proof)
            .unwrap(),
        EligibleEmptyThreadOutcomeAudit::Conflict
    );
    assert!(
        second_storage
            .prepare_eligible_empty_thread_outcome(
                &second_store,
                candidate(&first_store, &first_storage),
                summary(&second_store, &second_storage, 3)
            )
            .is_err()
    );
    assert!(
        second_storage
            .reuse_empty_thread_with_catalog_predecessor(
                &second_store,
                candidate(&first_store, &first_storage),
                summary(&second_store, &second_storage, 3),
                None,
            )
            .is_err()
    );
}

#[test]
fn source_drift_after_validation_preparation_cannot_publish_coupled_state_effect() {
    let home = TestHome::new("eligible-writer-source-fence");
    let (store, storage) = open_store(&home);
    create(&store, &storage, 3);
    let validation = storage
        .validate_eligible_empty_thread(&store, candidate(&store, &storage))
        .unwrap();
    change_activity(&store, &storage, 10);
    let revision = store.home_revision().unwrap();
    let state = BerylState::reacquire(&store).unwrap();
    let session_revision = state.session().revision(&store).unwrap();
    let mut command = HomeCommand::new(revision);
    command.add_validation(validation).unwrap();
    command.add(window_mutation(&store)).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(state.session().revision(&store).unwrap(), session_revision);
}
