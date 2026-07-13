pub use beryl_state::{RecordRevision, UnixMillis};

#[allow(dead_code)]
#[path = "../src/catalog.rs"]
mod catalog;

use beryl_home_store::{
    CommandError, CursorReadLimits, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    ReadError,
};
use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, PathFlavor, RootId, RuntimeId, SyndicThreadId,
    ThreadRevision, WindowId,
};
use catalog::{
    CATALOG_MAX_STORED_RECENCY_BYTES, CatalogArchiveSummary, CatalogAvailabilitySummary,
    CatalogClaimKind, CatalogClaimSummary, CatalogExecutionSummary, CatalogFacts, CatalogFreshness,
    CatalogLineageSummary, CatalogMutationError, CatalogPointReadLimit, CatalogReadError,
    CatalogRowExpectation, CatalogSearchFields, CatalogSourceRevisions, CatalogState,
    CatalogTitleCandidate, CatalogTitleFacts, CatalogTitleSource, MarkCatalogRowStale,
    PublishCatalogRow,
};

fn open(path: &std::path::Path) -> (HomeStore, CatalogState) {
    let mut store =
        HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = CatalogState::register(&mut store).unwrap();
    (store, state)
}

fn execute(
    store: &HomeStore,
    state: CatalogState,
    mutation: impl FnOnce(beryl_model::DomainRevision) -> beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(mutation(state.revision(store).unwrap()))
        .unwrap();
    store.execute(command).map(|_| ())
}

fn sources(revision: u64, claimed: bool) -> CatalogSourceRevisions {
    CatalogSourceRevisions::new(
        ThreadRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        claimed.then(|| ClaimRevision::new(revision).unwrap()),
    )
}

fn facts(
    thread_byte: u8,
    source_revision: u64,
    activity: u64,
    generated: Option<&str>,
    syndic: Option<&str>,
    claimed: bool,
) -> CatalogFacts {
    let title = |text: &str| {
        CatalogTitleCandidate::new(text, ThreadRevision::new(source_revision).unwrap()).unwrap()
    };
    let titles = CatalogTitleFacts::new(generated.map(title), syndic.map(title));
    let execution = CatalogExecutionSummary::new(
        RuntimeId::from_bytes([10; 16]),
        RootId::from_bytes([11; 16]),
        "Host",
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Work\beryl").unwrap(),
        CatalogAvailabilitySummary::new(Availability::Available, Availability::Available),
    )
    .unwrap();
    let claim = if claimed {
        CatalogClaimSummary::claimed(
            WindowId::from_bytes([thread_byte; 16]),
            CatalogClaimKind::Active,
        )
    } else {
        CatalogClaimSummary::Unclaimed
    };
    CatalogFacts::new(
        titles,
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(activity),
        claim,
        CatalogLineageSummary::TopLevel,
        CatalogSearchFields::from_admitted_normalized(
            generated.or(syndic).unwrap_or("untitled").to_lowercase(),
            "host",
            r"c:\codex\codex.exe",
            r"c:\work\beryl",
        )
        .unwrap(),
    )
}

#[allow(clippy::too_many_arguments)]
fn publish(
    store: &HomeStore,
    state: CatalogState,
    thread_byte: u8,
    expectation: CatalogRowExpectation,
    source_revision: u64,
    activity: u64,
    generated: Option<&str>,
    syndic: Option<&str>,
) -> Result<(), CommandError> {
    let thread_id = SyndicThreadId::from_bytes([thread_byte; 16]);
    let mutation = PublishCatalogRow::new(
        thread_id,
        expectation,
        sources(source_revision, false),
        facts(
            thread_byte,
            source_revision,
            activity,
            generated,
            syndic,
            false,
        ),
    )
    .unwrap();
    execute(store, state, |revision| state.publish(revision, mutation))
}

fn contributor_source<T: std::error::Error + 'static>(error: &CommandError) -> Option<&T> {
    match error {
        CommandError::ContributorValidation { source, .. }
        | CommandError::ContributorAssembly { source, .. } => source.downcast_ref::<T>(),
        _ => None,
    }
}

#[test]
fn title_precedence_and_caller_admitted_search_are_exact() {
    let titles = CatalogTitleFacts::new(
        Some(CatalogTitleCandidate::new("Generated", ThreadRevision::new(2).unwrap()).unwrap()),
        Some(CatalogTitleCandidate::new("History", ThreadRevision::new(1).unwrap()).unwrap()),
    );
    assert_eq!(titles.display_title(), "Generated");
    assert_eq!(titles.display_source(), CatalogTitleSource::Generated);

    let history = CatalogTitleFacts::new(None, titles.syndic().cloned());
    assert_eq!(history.display_title(), "History");
    assert_eq!(history.display_source(), CatalogTitleSource::Syndic);

    let untitled = CatalogTitleFacts::default();
    assert_eq!(untitled.display_title(), "Untitled");
    assert_eq!(untitled.display_source(), CatalogTitleSource::Untitled);

    let admitted = CatalogSearchFields::from_admitted_normalized(
        "already folded",
        "host",
        r"c:\codex\codex.exe",
        r"c:\work\beryl",
    )
    .unwrap();
    assert_eq!(admitted.title(), "already folded");
}

#[test]
fn recent_first_order_is_stable_and_index_moves_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    for thread in [3, 1, 2] {
        publish(
            &store,
            state,
            thread,
            CatalogRowExpectation::Missing,
            1,
            100,
            None,
            Some("History"),
        )
        .unwrap();
    }

    let limits = CursorReadLimits::new(10, 10 * CATALOG_MAX_STORED_RECENCY_BYTES).unwrap();
    let page = state.recency_page(&store, None, limits).unwrap();
    let ids: Vec<_> = page.rows().iter().map(|row| row.thread_id()).collect();
    assert_eq!(
        ids,
        [1, 2, 3].map(|byte| SyndicThreadId::from_bytes([byte; 16]))
    );
    let first_page = state
        .recency_page(
            &store,
            None,
            CursorReadLimits::new(2, 2 * CATALOG_MAX_STORED_RECENCY_BYTES).unwrap(),
        )
        .unwrap();
    assert!(first_page.has_more());
    let second_page = state
        .recency_page(&store, first_page.next_after(), limits)
        .unwrap();
    assert_eq!(second_page.rows().len(), 1);
    assert_eq!(
        second_page.rows()[0].thread_id(),
        SyndicThreadId::from_bytes([3; 16])
    );

    let thread_two = state
        .row(
            &store,
            SyndicThreadId::from_bytes([2; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    publish(
        &store,
        state,
        2,
        CatalogRowExpectation::Revision(thread_two.row().revision()),
        2,
        200,
        Some("Generated"),
        Some("History"),
    )
    .unwrap();

    let page = state.recency_page(&store, None, limits).unwrap();
    assert_eq!(page.rows().len(), 3, "old recency key must be deleted");
    assert_eq!(
        page.rows()[0].thread_id(),
        SyndicThreadId::from_bytes([2; 16])
    );
    assert_eq!(page.rows()[0].display_title(), "Generated");
}

#[test]
fn point_and_page_reads_report_exact_stored_byte_costs() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    publish(
        &store,
        state,
        1,
        CatalogRowExpectation::Missing,
        1,
        100,
        None,
        None,
    )
    .unwrap();

    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let generous = state
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    let exact = generous.stored_bytes();
    assert!(exact > 0);
    assert_eq!(
        state
            .row(
                &store,
                thread_id,
                CatalogPointReadLimit::new(exact).unwrap()
            )
            .unwrap()
            .unwrap()
            .stored_bytes(),
        exact
    );
    let error = state
        .row(
            &store,
            thread_id,
            CatalogPointReadLimit::new(exact - 1).unwrap(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        CatalogReadError::Read(ReadError::BoundExceeded { .. })
    ));

    let page = state
        .recency_page(
            &store,
            None,
            CursorReadLimits::new(1, CATALOG_MAX_STORED_RECENCY_BYTES).unwrap(),
        )
        .unwrap();
    assert_eq!(page.rows().len(), 1);
    let exact_page = page.stored_bytes();
    assert!(
        state
            .recency_page(&store, None, CursorReadLimits::new(1, exact_page).unwrap(),)
            .is_ok()
    );
}

#[test]
fn stale_marker_preserves_sources_and_recency_copy_across_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    publish(
        &store,
        state,
        1,
        CatalogRowExpectation::Missing,
        1,
        100,
        None,
        Some("History"),
    )
    .unwrap();
    let current = state
        .row(
            &store,
            SyndicThreadId::from_bytes([1; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    let sources = current.row().sources();
    let command = MarkCatalogRowStale::new(current.row().thread_id(), current.row().revision());
    execute(&store, state, |revision| {
        state.mark_stale(revision, command)
    })
    .unwrap();

    let page = state
        .recency_page(
            &store,
            None,
            CursorReadLimits::new(2, 2 * CATALOG_MAX_STORED_RECENCY_BYTES).unwrap(),
        )
        .unwrap();
    assert_eq!(page.rows()[0].freshness(), CatalogFreshness::Stale);
    assert_eq!(page.rows()[0].sources(), sources);
    drop(store);

    let (reopened, reopened_state) = open(directory.path());
    let row = reopened_state
        .row(
            &reopened,
            SyndicThreadId::from_bytes([1; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(row.row().freshness(), CatalogFreshness::Stale);
    assert_eq!(row.row().sources(), sources);
}

#[test]
fn publish_rejects_regressing_authoritative_source_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    publish(
        &store,
        state,
        1,
        CatalogRowExpectation::Missing,
        2,
        100,
        None,
        Some("History"),
    )
    .unwrap();
    let current = state
        .row(
            &store,
            SyndicThreadId::from_bytes([1; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    let error = publish(
        &store,
        state,
        1,
        CatalogRowExpectation::Revision(current.row().revision()),
        1,
        101,
        None,
        Some("Older"),
    )
    .unwrap_err();
    assert!(matches!(
        contributor_source::<CatalogMutationError>(&error),
        Some(CatalogMutationError::SourceRevisionRegressed {
            kind: "Syndic thread"
        })
    ));
}

#[test]
fn claim_fact_and_source_revision_must_have_the_same_shape() {
    let mismatch = PublishCatalogRow::new(
        SyndicThreadId::from_bytes([1; 16]),
        CatalogRowExpectation::Missing,
        sources(1, true),
        facts(1, 1, 100, None, None, false),
    );
    assert!(mismatch.is_err());
}

#[test]
fn reopen_rejects_a_recency_copy_that_disagrees_with_its_row() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    for thread in [1, 2] {
        publish(
            &store,
            state,
            thread,
            CatalogRowExpectation::Missing,
            1,
            100,
            None,
            Some("History"),
        )
        .unwrap();
    }
    let read = |thread| {
        state
            .row(
                &store,
                SyndicThreadId::from_bytes([thread; 16]),
                CatalogPointReadLimit::schema_maximum(),
            )
            .unwrap()
            .unwrap()
            .row()
            .clone()
    };
    let first = read(1);
    let second = read(2);
    execute(&store, state, |revision| {
        state.corrupt_recency_copy_for_test(revision, first.recency_cursor(), second)
    })
    .unwrap();
    drop(store);

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let error = match CatalogState::register(&mut reopened) {
        Ok(_) => panic!("mismatched recency copy must fail reopen validation"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("copies disagree"));
}
