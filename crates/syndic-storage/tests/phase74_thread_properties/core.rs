use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{
    ExecutionBinding, PathFlavor, ProjectionRevision, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicPathDigest, ThreadRevision,
};
use syndic_storage::test_faults::{
    FixtureBatch, FixtureDelete, FixtureRecord, PhysicalFamily, thread_attributes_with_revision,
    thread_usage_with_revision,
};
use syndic_storage::{
    CreateThread, SyndicPointReadLimit, SyndicStorage, ThreadAttributesRevision,
    ThreadCatalogSourceWitnesses, ThreadCatalogSummaryRecord, ThreadCatalogTitle,
    ThreadCatalogTitleSource, ThreadCreationStatus, ThreadUsageRecord, ThreadUsageRevision,
};

use crate::support::{TestHome, commit, draft_id, id, open, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
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
        outcome => panic!("expected clean thread-properties command, got {outcome:?}"),
    }
}

fn other_execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([0x74; 16]),
        RootId::from_bytes([0x75; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase74-other",
        )
        .unwrap(),
    )
}

#[test]
fn ordinary_creation_publishes_all_properties_and_reconciles_execution_exactly() {
    let home = TestHome::new("phase74-ordinary-properties");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    let execution = crate::support::exact_cas::execution_binding();
    let creation = CreateThread::ordinary(thread, draft, execution.clone(), timestamp(1));

    execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
    );
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Exact
    );
    assert_eq!(
        storage
            .thread_creation_status(
                &store,
                &CreateThread::ordinary(thread, draft, other_execution(), timestamp(1)),
                limit(),
            )
            .unwrap(),
        ThreadCreationStatus::Collision
    );
    assert_eq!(
        storage
            .thread_execution(&store, thread, limit())
            .unwrap()
            .unwrap()
            .execution(),
        &execution
    );
    assert_eq!(
        storage
            .thread_attributes(&store, thread, limit())
            .unwrap()
            .unwrap()
            .revision(),
        ThreadAttributesRevision::FIRST
    );
    assert_eq!(
        storage
            .thread_usage(&store, thread, limit())
            .unwrap()
            .unwrap(),
        ThreadUsageRecord::empty(thread)
    );
    assert!(
        storage
            .thread_catalog_summary(&store, thread, limit())
            .unwrap()
            .is_some()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn from_tail_inherits_the_source_canonical_execution() {
    let home = TestHome::new("phase74-inherited-execution");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    crate::support::seed_populated(&store, storage);
    let source = storage
        .thread_tail(&store, id(30), limit())
        .unwrap()
        .unwrap();
    let expected = source.execution().clone();
    let child = id(80);
    let creation = CreateThread::from_tail(
        child,
        SyndicDraftId::from_bytes([81; 16]),
        timestamp(20),
        source,
    )
    .unwrap();
    execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), creation),
    );
    assert_eq!(
        storage
            .thread_execution(&store, child, limit())
            .unwrap()
            .unwrap()
            .execution(),
        &expected
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn missing_or_orphan_properties_and_child_execution_disagreement_are_rejected() {
    let home = TestHome::new("phase74-missing-property");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let creation = CreateThread::ordinary(
        id(10),
        draft_id(11),
        crate::support::exact_cas::execution_binding(),
        timestamp(1),
    );
    execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), creation),
    );
    let mut deletion = FixtureBatch::new();
    deletion.delete(FixtureDelete::ThreadUsage(id(10))).unwrap();
    commit(&store, storage, deletion);
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("usage record is missing")
    );

    let orphan_home = TestHome::new("phase74-orphan-property");
    let mut orphan_store = open(orphan_home.path());
    let orphan_storage = SyndicStorage::register(&mut orphan_store).unwrap();
    commit(
        &orphan_store,
        orphan_storage,
        crate::support::batch([FixtureRecord::ThreadUsage(ThreadUsageRecord::empty(id(12)))]),
    );
    assert!(
        orphan_store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("orphan thread property")
    );

    let child_home = TestHome::new("phase74-child-execution-conflict");
    let mut child_store = open(child_home.path());
    let child_storage = SyndicStorage::register(&mut child_store).unwrap();
    crate::support::seed_populated(&child_store, child_storage);
    commit(
        &child_store,
        child_storage,
        crate::support::batch([FixtureRecord::ThreadExecution(
            syndic_storage::ThreadExecutionRecord::new(id(36), other_execution()),
        )]),
    );
    assert!(
        child_store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("does not inherit")
    );
}

#[test]
fn impossible_property_revisions_and_future_catalog_witnesses_are_rejected() {
    for (name, replacement) in [
        (
            "attributes-gap",
            FixtureRecord::ThreadAttributes(thread_attributes_with_revision(
                &syndic_storage::ThreadAttributesRecord::ordinary(id(20)),
                ThreadAttributesRevision::new(2).unwrap(),
            )),
        ),
        (
            "usage-gap",
            FixtureRecord::ThreadUsage(thread_usage_with_revision(
                &ThreadUsageRecord::empty(id(20)),
                ThreadUsageRevision::new(2).unwrap(),
            )),
        ),
    ] {
        let home = TestHome::new(&format!("phase74-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    id(20),
                    draft_id(21),
                    crate::support::exact_cas::execution_binding(),
                    timestamp(1),
                ),
            ),
        );
        commit(&store, storage, crate::support::batch([replacement]));
        assert!(
            store
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .is_err()
        );
    }

    let home = TestHome::new("phase74-future-catalog-witness");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                id(30),
                draft_id(31),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let current = storage
        .thread_catalog_summary(&store, id(30), limit())
        .unwrap()
        .unwrap();
    let future = ThreadCatalogSummaryRecord::new(
        current.thread_id(),
        ProjectionRevision::new(2).unwrap(),
        current.title().cloned(),
        current.execution().clone(),
        current.archive(),
        current.last_activity_at(),
        current.complete(),
        current.parent_thread_id(),
        current.lineage_depth(),
        current.lineage_digest(),
        ThreadCatalogSourceWitnesses::new(
            current.sources().attributes_revision(),
            ProjectionRevision::new(2).unwrap(),
            ThreadRevision::new(1).unwrap(),
            current.sources().history_selected_path_digest(),
            ThreadRevision::new(1).unwrap(),
        ),
    );
    commit(
        &store,
        storage,
        crate::support::batch([FixtureRecord::ThreadCatalogSummary(future)]),
    );
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("future")
    );
    assert!(PhysicalFamily::ALL.contains(&PhysicalFamily::ThreadCatalogSummaries));
}

#[test]
fn exact_catalog_history_revision_requires_matching_semantic_provenance() {
    let home = TestHome::new("phase74-corrupt-catalog-history-provenance");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                id(40),
                draft_id(41),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let current = storage
        .thread_catalog_summary(&store, id(40), limit())
        .unwrap()
        .unwrap();
    let sources = current.sources();
    let corrupt = ThreadCatalogSummaryRecord::new(
        current.thread_id(),
        current.revision().checked_next().unwrap(),
        current.title().cloned(),
        current.execution().clone(),
        current.archive(),
        current.last_activity_at(),
        current.complete(),
        current.parent_thread_id(),
        current.lineage_depth(),
        current.lineage_digest(),
        ThreadCatalogSourceWitnesses::new(
            sources.attributes_revision(),
            sources.history_summary_revision(),
            sources.history_thread_revision(),
            SyndicPathDigest::from_bytes([0x74; 32]),
            sources.thread_revision(),
        ),
    );
    commit(
        &store,
        storage,
        crate::support::batch([FixtureRecord::ThreadCatalogSummary(corrupt)]),
    );
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("history provenance")
    );
}

#[test]
fn generated_catalog_title_requires_a_current_canonical_attributes_source() {
    let home = TestHome::new("phase74-orphan-generated-catalog-title");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                id(50),
                draft_id(51),
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let current = storage
        .thread_catalog_summary(&store, id(50), limit())
        .unwrap()
        .unwrap();
    let corrupt = ThreadCatalogSummaryRecord::new(
        current.thread_id(),
        current.revision().checked_next().unwrap(),
        Some(
            ThreadCatalogTitle::new(
                "Unowned generated title",
                ThreadCatalogTitleSource::Generated,
            )
            .unwrap(),
        ),
        current.execution().clone(),
        current.archive(),
        current.last_activity_at(),
        current.complete(),
        current.parent_thread_id(),
        current.lineage_depth(),
        current.lineage_digest(),
        current.sources(),
    );
    commit(
        &store,
        storage,
        crate::support::batch([FixtureRecord::ThreadCatalogSummary(corrupt)]),
    );
    assert!(
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .to_string()
            .contains("no canonical attributes source")
    );
}
