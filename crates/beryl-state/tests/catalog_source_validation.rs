mod support;

use beryl_home_store::{CommandError, CommandOutcome, HomeCommand};
use beryl_model::{
    Availability, ProjectionRevision, RootId, RuntimeId, SyndicThreadId, UnavailableReason,
};
use beryl_state::{
    AvailabilitySnapshot, CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary, CatalogPointReadLimit,
    CatalogResolvedTitle, CatalogRowExpectation, CatalogSourceRevisions, PublishCatalogRow,
    RuntimeRootCatalogSource, RuntimeRootCatalogSourceError, SetRuntimeAvailability, UnixMillis,
};

use support::{contributor_source, create_host_runtime, execute, open};

fn facts(source: &RuntimeRootCatalogSource, activity: u64) -> CatalogFacts {
    let runtime = source.runtime();
    let root = source.root();
    CatalogFacts::new(
        CatalogResolvedTitle::history_derived("Source-fenced row").unwrap(),
        CatalogExecutionSummary::new(
            runtime.runtime_id(),
            root.root_id(),
            runtime.environment_label(),
            runtime.canonical_executable().clone(),
            root.display_path().clone(),
            CatalogAvailabilitySummary::new(
                runtime.availability().availability(),
                root.availability().availability(),
            ),
        )
        .unwrap(),
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(activity),
        true,
        CatalogClaimSummary::Unclaimed,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap()
}

fn sources(source: &RuntimeRootCatalogSource) -> CatalogSourceRevisions {
    CatalogSourceRevisions::new(
        ProjectionRevision::new(1).unwrap(),
        source.runtime().revision(),
        source.root().revision(),
        None,
    )
}

#[test]
fn runtime_root_and_unclaimed_sources_guard_one_catalog_publication() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    create_host_runtime(&store, state, 1, 2, r"C:\Codex\codex.exe", r"C:\Work\beryl");

    let thread_id = SyndicThreadId::from_bytes([3; 16]);
    let runtime_revision = state.runtime_roots().revision(&store).unwrap();
    let runtime_source = state
        .runtime_roots()
        .catalog_source(
            &store,
            RuntimeId::from_bytes([1; 16]),
            RootId::from_bytes([2; 16]),
        )
        .unwrap();
    let session_revision = state.session().revision(&store).unwrap();
    let claim_source = state
        .session()
        .thread_claim_catalog_source(&store, thread_id)
        .unwrap();
    assert_eq!(claim_source.claim(), None);

    let mutation = PublishCatalogRow::new(
        thread_id,
        CatalogRowExpectation::Missing,
        sources(&runtime_source),
        facts(&runtime_source, 10),
    )
    .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            state
                .catalog()
                .publish(state.catalog().revision(&store).unwrap(), mutation),
        )
        .unwrap();
    command
        .add_validation(
            state
                .runtime_roots()
                .validate_catalog_source(runtime_revision, runtime_source.clone()),
        )
        .unwrap();
    command
        .add_validation(
            state
                .session()
                .validate_thread_claim_catalog_source(session_revision, claim_source),
        )
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed catalog-source command, got {outcome:?}"),
    }

    let row = state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    assert!(row.facts().complete());
    assert_eq!(row.facts().search().title(), "source-fenced row");
}

#[test]
fn exact_runtime_root_validator_rejects_a_same_revision_stale_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    create_host_runtime(&store, state, 1, 2, r"C:\Codex\codex.exe", r"C:\Work\beryl");
    let stale_source = state
        .runtime_roots()
        .catalog_source(
            &store,
            RuntimeId::from_bytes([1; 16]),
            RootId::from_bytes([2; 16]),
        )
        .unwrap();

    let outcome = execute(
        &store,
        state.runtime_roots().set_runtime_availability(
            state.runtime_roots().revision(&store).unwrap(),
            SetRuntimeAvailability::new(
                stale_source.runtime().runtime_id(),
                stale_source.runtime().revision(),
                AvailabilitySnapshot::observed(
                    Availability::Unavailable(UnavailableReason::NotFound),
                    UnixMillis::new(20),
                )
                .unwrap(),
            ),
        ),
    );
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed source update, got {outcome:?}"),
    }

    let thread_id = SyndicThreadId::from_bytes([4; 16]);
    let mutation = PublishCatalogRow::new(
        thread_id,
        CatalogRowExpectation::Missing,
        sources(&stale_source),
        facts(&stale_source, 10),
    )
    .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            state
                .catalog()
                .publish(state.catalog().revision(&store).unwrap(), mutation),
        )
        .unwrap();
    command
        .add_validation(state.runtime_roots().validate_catalog_source(
            state.runtime_roots().revision(&store).unwrap(),
            stale_source,
        ))
        .unwrap();

    let error = match store.execute(command) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected catalog-source command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<RuntimeRootCatalogSourceError>(&error),
        Some(RuntimeRootCatalogSourceError::SourceChanged("runtime"))
    ));
    assert!(matches!(error, CommandError::ContributorValidation { .. }));
    assert!(state
        .catalog()
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .is_none());
}
