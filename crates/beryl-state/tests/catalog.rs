pub use beryl_state::{RecordRevision, StatePage, UnixMillis};

#[path = "../src/reconciliation.rs"]
mod reconciliation;

#[allow(dead_code)]
#[path = "../src/catalog.rs"]
mod catalog;

use beryl_home_store::{
    CommandError, CommandOutcome, CursorReadLimits, DomainReconciliation, HomeCommand,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, MutationContribution, ReadError,
    ReconciliationResolution, WholeHomeScrubTrigger,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    AdmittedHostPath, Availability, ClaimRevision, HomeRevision, PathFlavor, ProjectionRevision,
    RootId, RuntimeId, SyndicPathDigest, SyndicThreadId, WindowId,
};
use catalog::{
    CATALOG_MAX_STORED_RECENCY_BYTES, CATALOG_NORMALIZATION_PROFILE, CATALOG_QUERY_MAX_BYTES,
    CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimKind, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFacts, CatalogFreshness, CatalogLineageSummary,
    CatalogMutationError, CatalogNormalizationProfile, CatalogNormalizedQuery,
    CatalogPointReadLimit, CatalogReadError, CatalogResolvedTitle, CatalogRow,
    CatalogRowExpectation, CatalogSourceRevisions, CatalogState, CatalogTitleSource,
    MarkCatalogRowStale, PublishCatalogRow,
};

fn open(path: &std::path::Path) -> (HomeStore, CatalogState) {
    let mut store =
        HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let state = CatalogState::register(&mut store).unwrap();
    (store, state)
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> (HomeStore, CatalogState) {
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = CatalogState::register(&mut store).unwrap();
    (store, state)
}

fn execute_at(
    store: &HomeStore,
    expected_home_revision: HomeRevision,
    contribution: MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(expected_home_revision);
    command.add(contribution).unwrap();
    store.execute(command)
}

macro_rules! assert_committed {
    ($outcome:expr) => {
        assert!(matches!(
            $outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    };
}

fn execute(
    store: &HomeStore,
    state: &CatalogState,
    mutation: impl FnOnce(beryl_model::DomainRevision) -> beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(mutation(state.revision(store).unwrap()))
        .unwrap();
    store.execute(command)
}

fn sources(revision: u64, claimed: bool) -> CatalogSourceRevisions {
    CatalogSourceRevisions::new(
        ProjectionRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        RecordRevision::new(revision).unwrap(),
        claimed.then(|| ClaimRevision::new(revision).unwrap()),
    )
}

fn facts(
    thread_byte: u8,
    activity: u64,
    title: CatalogResolvedTitle,
    claimed: bool,
) -> CatalogFacts {
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
        title,
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(activity),
        true,
        claim,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn publish(
    store: &HomeStore,
    state: &CatalogState,
    thread_byte: u8,
    expectation: CatalogRowExpectation,
    source_revision: u64,
    activity: u64,
    title: CatalogResolvedTitle,
) -> CommandOutcome {
    let thread_id = SyndicThreadId::from_bytes([thread_byte; 16]);
    let mutation = PublishCatalogRow::new(
        thread_id,
        expectation,
        sources(source_revision, false),
        facts(thread_byte, activity, title, false),
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

fn seed_rows(store: &HomeStore, state: &CatalogState, count: u8) -> Vec<CatalogRow> {
    for thread in 1..=count {
        assert_committed!(publish(
            store,
            state,
            thread,
            CatalogRowExpectation::Missing,
            1,
            u64::from(thread) * 100,
            CatalogResolvedTitle::history_derived(format!("History {thread}")).unwrap(),
        ));
    }
    (1..=count)
        .map(|thread| {
            state
                .row(
                    store,
                    SyndicThreadId::from_bytes([thread; 16]),
                    CatalogPointReadLimit::schema_maximum(),
                )
                .unwrap()
                .unwrap()
        })
        .collect()
}

fn indeterminate_handle(outcome: CommandOutcome) -> beryl_home_store::ReconciliationHandle {
    match outcome {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        outcome => panic!("expected indeterminate catalog contribution, got {outcome:?}"),
    }
}

#[test]
fn catalog_reconciliation_exact_new_reconstructs_a_receipt_without_domain_validation() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let rows = seed_rows(&store, &state, 3);

    assert_committed!(execute(&store, &state, |revision| {
        state.corrupt_recency_copy_for_test(revision, rows[0].recency_cursor(), rows[1].clone())
    }));

    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = indeterminate_handle(execute_at(
        &store,
        home_before,
        state.corrupt_recency_copy_for_test(
            domain_before,
            rows[2].recency_cursor(),
            rows[0].clone(),
        ),
    ));

    match store.reconcile(&handle).unwrap() {
        ReconciliationResolution::ExactNew { receipt } => {
            assert_eq!(receipt.home_revision(), home_before.checked_next().unwrap());
        }
        outcome => panic!("expected exact-new receipt reconstruction, got {outcome:?}"),
    }
    store.close().unwrap();
}

#[test]
fn catalog_reconciliation_exact_old_releases_the_scope_without_a_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let rows = seed_rows(&store, &state, 2);
    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = indeterminate_handle(execute_at(
        &store,
        home_before,
        state.corrupt_recency_copy_for_test(
            domain_before,
            rows[0].recency_cursor(),
            rows[1].clone(),
        ),
    ));
    assert_committed!(execute_at(
        &store,
        home_before.checked_next().unwrap(),
        state.corrupt_recency_copy_for_test(
            domain_before.checked_next().unwrap(),
            rows[0].recency_cursor(),
            rows[0].clone(),
        ),
    ));
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactOld
    );
    store.close().unwrap();
}

#[test]
fn catalog_reconciliation_mixed_natural_records_seal_a_collision() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let old = seed_rows(&store, &state, 1).pop().unwrap();
    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();
    let update = PublishCatalogRow::new(
        old.thread_id(),
        CatalogRowExpectation::Revision(old.revision()),
        sources(2, false),
        facts(
            1,
            200,
            CatalogResolvedTitle::generated("Replacement").unwrap(),
            false,
        ),
    )
    .unwrap();

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = indeterminate_handle(execute_at(
        &store,
        home_before,
        state.publish(domain_before, update),
    ));
    assert_committed!(execute_at(
        &store,
        home_before.checked_next().unwrap(),
        state.corrupt_recency_copy_for_test(
            domain_before.checked_next().unwrap(),
            old.recency_cursor(),
            old,
        ),
    ));
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}

#[test]
fn catalog_reconciliation_neither_natural_record_seals_a_collision() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let rows = seed_rows(&store, &state, 3);
    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = indeterminate_handle(execute_at(
        &store,
        home_before,
        state.corrupt_recency_copy_for_test(
            domain_before,
            rows[0].recency_cursor(),
            rows[1].clone(),
        ),
    ));
    assert_committed!(execute_at(
        &store,
        home_before.checked_next().unwrap(),
        state.corrupt_recency_copy_for_test(
            domain_before.checked_next().unwrap(),
            rows[0].recency_cursor(),
            rows[2].clone(),
        ),
    ));
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}

#[test]
fn catalog_reconciliation_dual_side_record_seals_a_collision() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = open_with_faults(directory.path(), faults.clone());
    let row = seed_rows(&store, &state, 1).pop().unwrap();
    let home_before = store.home_revision().unwrap();
    let domain_before = state.revision(&store).unwrap();

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = indeterminate_handle(execute_at(
        &store,
        home_before,
        state.corrupt_recency_copy_for_test(domain_before, row.recency_cursor(), row),
    ));
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}

#[test]
fn reconciliation_classification_never_revives_a_collision() {
    let mut classification = reconciliation::ReconciliationClassification::new();
    classification.observe_matches(true, true);
    classification.observe_matches(false, true);
    assert_eq!(classification.finish(), DomainReconciliation::Collision);
}

#[test]
fn resolved_title_and_catalog_owned_search_are_exact() {
    let generated = CatalogResolvedTitle::generated("Generated").unwrap();
    assert_eq!(generated.text(), Some("Generated"));
    assert_eq!(generated.source(), CatalogTitleSource::Generated);

    let history = CatalogResolvedTitle::history_derived("History").unwrap();
    assert_eq!(history.text(), Some("History"));
    assert_eq!(history.source(), CatalogTitleSource::HistoryDerived);

    let absent = CatalogResolvedTitle::absent();
    assert_eq!(absent.text(), None);
    assert_eq!(absent.source(), CatalogTitleSource::Absent);
    assert!(CatalogResolvedTitle::generated("---").is_err());
    assert!(CatalogResolvedTitle::history_derived(" ---").is_ok());
    assert!(CatalogResolvedTitle::history_derived("History ").is_err());
    assert!(CatalogResolvedTitle::history_derived("-".repeat(80)).is_ok());
    assert!(CatalogResolvedTitle::history_derived("-".repeat(81)).is_err());

    let normalized_facts = facts(
        1,
        1,
        CatalogResolvedTitle::history_derived("Straße").unwrap(),
        false,
    );
    assert_eq!(normalized_facts.search().title(), "strasse");
    assert_eq!(
        CatalogNormalizedQuery::new("STRASSE").unwrap().as_str(),
        "strasse"
    );
    assert!(CatalogNormalizedQuery::new("\u{00ad}").unwrap().is_empty());
    assert!(
        CatalogNormalizedQuery::new("\u{00ad}".repeat(CATALOG_QUERY_MAX_BYTES / 2 + 1)).is_err()
    );
    assert!(CatalogNormalizedQuery::new("\u{fdfa}".repeat(2_000)).is_err());
    let profile: CatalogNormalizationProfile = CATALOG_NORMALIZATION_PROFILE;
    assert_eq!(profile.version(), 1);
    assert_eq!(profile.unicode_version(), (17, 0, 0));

    let absent = facts(2, 2, CatalogResolvedTitle::absent(), false);
    assert_eq!(absent.search().title(), "");
    assert!(
        CatalogFacts::new(
            CatalogResolvedTitle::history_derived("\u{fdfa}".repeat(80)).unwrap(),
            absent.execution().clone(),
            CatalogArchiveSummary::Ordinary,
            UnixMillis::new(2),
            true,
            CatalogClaimSummary::Unclaimed,
            CatalogLineageSummary::TopLevel,
        )
        .is_err()
    );

    let lineage = CatalogLineageSummary::descendant(
        SyndicThreadId::from_bytes([9; 16]),
        u64::MAX,
        SyndicPathDigest::from_bytes([7; 32]),
    )
    .unwrap();
    assert_eq!(lineage.depth(), u64::MAX);
    assert_eq!(
        lineage.path_digest(),
        Some(SyndicPathDigest::from_bytes([7; 32]))
    );
}

#[test]
fn recent_first_order_is_stable_and_index_moves_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    for thread in [3, 1, 2] {
        assert_committed!(publish(
            &store,
            &state,
            thread,
            CatalogRowExpectation::Missing,
            1,
            100,
            CatalogResolvedTitle::history_derived("History").unwrap(),
        ));
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
    assert_committed!(publish(
        &store,
        &state,
        2,
        CatalogRowExpectation::Revision(thread_two.revision()),
        2,
        200,
        CatalogResolvedTitle::generated("Generated").unwrap(),
    ));

    let page = state.recency_page(&store, None, limits).unwrap();
    assert_eq!(page.rows().len(), 3, "old recency key must be deleted");
    assert_eq!(
        page.rows()[0].thread_id(),
        SyndicThreadId::from_bytes([2; 16])
    );
    assert_eq!(page.rows()[0].title().text(), Some("Generated"));
    assert_eq!(page.rows()[0].title_source(), CatalogTitleSource::Generated);
}

#[test]
fn point_limits_and_page_costs_cover_stored_and_decoded_bytes() {
    assert_eq!(
        CatalogPointReadLimit::schema_maximum().max_bytes(),
        4 + 256 * 1024,
        "the point limit covers only the stored value envelope, not its request key",
    );

    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    assert_committed!(publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Missing,
        1,
        100,
        CatalogResolvedTitle::absent(),
    ));

    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let row = state
        .row(&store, thread_id, CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    assert_eq!(row.thread_id(), thread_id);
    assert!(row.facts().complete());
    assert_eq!(row.facts().search().title(), "");
    let error = state
        .row(&store, thread_id, CatalogPointReadLimit::new(1).unwrap())
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
    assert!(page.stored_bytes() > 0);
    assert!(page.decoded_bytes() > 0);
    let exact_page = page.stored_bytes().max(page.decoded_bytes());
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
    let visible_title = "Stra\u{00df}e";
    assert_committed!(publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Missing,
        1,
        100,
        CatalogResolvedTitle::history_derived(visible_title).unwrap(),
    ));
    let current = state
        .row(
            &store,
            SyndicThreadId::from_bytes([1; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(current.title().text(), Some(visible_title));
    assert_eq!(current.facts().search().title(), "strasse");
    let sources = current.sources();
    let command = MarkCatalogRowStale::new(current.thread_id(), current.revision());
    assert_committed!(execute(&store, &state, |revision| {
        state.mark_stale(revision, command)
    }));

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
    assert_eq!(row.freshness(), CatalogFreshness::Stale);
    assert_eq!(row.sources(), sources);
    assert_eq!(row.title().text(), Some(visible_title));
    assert_eq!(row.facts().search().title(), "strasse");
    assert_eq!(
        CatalogNormalizedQuery::new("STRASSE").unwrap().as_str(),
        row.facts().search().title(),
    );
}

#[test]
fn publish_rejects_regressing_authoritative_source_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    assert_committed!(publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Missing,
        2,
        100,
        CatalogResolvedTitle::history_derived("History").unwrap(),
    ));
    let current = state
        .row(
            &store,
            SyndicThreadId::from_bytes([1; 16]),
            CatalogPointReadLimit::schema_maximum(),
        )
        .unwrap()
        .unwrap();
    let error = match publish(
        &store,
        &state,
        1,
        CatalogRowExpectation::Revision(current.revision()),
        1,
        101,
        CatalogResolvedTitle::history_derived("Older").unwrap(),
    ) {
        CommandOutcome::NotCommitted { evidence } => evidence,
        outcome => panic!("expected rejected catalog command, got {outcome:?}"),
    };
    assert!(matches!(
        contributor_source::<CatalogMutationError>(&error),
        Some(CatalogMutationError::SourceRevisionRegressed {
            kind: "Syndic catalog summary"
        })
    ));
}

#[test]
fn claim_fact_and_source_revision_must_have_the_same_shape() {
    let mismatch = PublishCatalogRow::new(
        SyndicThreadId::from_bytes([1; 16]),
        CatalogRowExpectation::Missing,
        sources(1, true),
        facts(1, 100, CatalogResolvedTitle::absent(), false),
    );
    assert!(mismatch.is_err());
}

#[test]
fn routine_reopen_defers_a_dormant_recency_copy_disagreement_to_explicit_scrub() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = open(directory.path());
    for thread in [1, 2] {
        assert_committed!(publish(
            &store,
            &state,
            thread,
            CatalogRowExpectation::Missing,
            1,
            100,
            CatalogResolvedTitle::history_derived("History").unwrap(),
        ));
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
    };
    let first = read(1);
    let second = read(2);
    assert_committed!(execute(&store, &state, |revision| {
        state.corrupt_recency_copy_for_test(revision, first.recency_cursor(), second)
    }));
    drop(store);

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let routine = CatalogState::register(&mut reopened)
        .expect("routine catalog registration must not scan dormant recency copies");
    assert_eq!(
        routine
            .row(
                &reopened,
                SyndicThreadId::from_bytes([1; 16]),
                CatalogPointReadLimit::schema_maximum(),
            )
            .unwrap(),
        Some(first)
    );
    let error = reopened
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .validation_error()
            .to_string()
            .contains("copies disagree")
    );
}
