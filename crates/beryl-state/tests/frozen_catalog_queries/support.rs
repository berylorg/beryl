use super::*;

pub fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
pub fn runtime(seed: u8) -> RuntimeId {
    RuntimeId::from_bytes([seed; 16])
}
pub fn root(seed: u8) -> RootId {
    RootId::from_bytes([seed; 16])
}
pub fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}
pub fn configure(store: &HomeStore, state: &BerylState) {
    support::create_host_runtime(store, state, 10, 11, r"C:\Codex\codex.exe", r"C:\Work\one");
    support::create_host_runtime(store, state, 20, 21, r"C:\Other\codex.exe", r"C:\Work\two");
}
pub fn publish(
    store: &HomeStore,
    state: &BerylState,
    seed: u8,
    runtime_seed: u8,
    root_seed: u8,
    title: &str,
    activity: u64,
) {
    let source = state
        .runtime_roots()
        .catalog_source(store, runtime(runtime_seed), root(root_seed))
        .unwrap();
    let old = state
        .catalog()
        .row(store, thread(seed), CatalogPointReadLimit::schema_maximum())
        .unwrap();
    let projection = old
        .as_ref()
        .map_or(1, |old| old.sources().syndic_summary().get() + 1);
    let sources = CatalogSourceRevisions::new(
        ProjectionRevision::new(projection).unwrap(),
        source.runtime().revision(),
        source.root().revision(),
        None,
    );
    let execution = CatalogExecutionSummary::new(
        runtime(runtime_seed),
        root(root_seed),
        source.runtime().environment_label(),
        source.runtime().canonical_executable().clone(),
        source.root().display_path().clone(),
        CatalogAvailabilitySummary::new(
            source.runtime().availability().availability(),
            source.root().availability().availability(),
        ),
    )
    .unwrap();
    let facts = CatalogFacts::new(
        CatalogResolvedTitle::generated(title).unwrap(),
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(activity),
        true,
        CatalogClaimSummary::Unclaimed,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap();
    let expected = old.map_or(CatalogRowExpectation::Missing, |row| {
        CatalogRowExpectation::Revision(row.revision())
    });
    committed(support::execute(
        store,
        state.catalog().publish(
            state.catalog().revision(store).unwrap(),
            PublishCatalogRow::new(thread(seed), expected, sources, facts).unwrap(),
        ),
    ));
}
pub fn owner(store: &HomeStore, state: &BerylState) -> CatalogQueryOwner {
    CatalogQueryOwner::new(
        state.catalog(),
        state.runtime_roots(),
        store.generation_identity().unwrap(),
    )
    .unwrap()
}
pub fn criteria(scope: CatalogQueryScope, search: &str) -> CatalogQueryCriteria {
    CatalogQueryCriteria::new(scope, CatalogNormalizedQuery::new(search).unwrap())
}
pub fn limit(items: usize) -> CatalogQueryPageLimit {
    CatalogQueryPageLimit::new(items, CATALOG_QUERY_PAGE_MAX_BYTES).unwrap()
}
pub fn open(
    store: &HomeStore,
    owner: &mut CatalogQueryOwner,
    scope: CatalogQueryScope,
    search: &str,
    items: usize,
) -> CatalogQueryOpened {
    owner
        .open(
            store,
            store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap(),
            criteria(scope, search),
            limit(items),
            &CommandCancellation::new(),
        )
        .unwrap()
}
pub fn ids(page: &CatalogQueryPage) -> Vec<SyndicThreadId> {
    page.rows()
        .iter()
        .map(|row| row.catalog().thread_id())
        .collect()
}
