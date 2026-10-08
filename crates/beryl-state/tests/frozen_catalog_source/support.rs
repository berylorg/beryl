use super::*;
use beryl_model::{AdmittedHostPath, Availability, PathFlavor, ProjectionRevision};

pub fn thread(seed: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([seed; 16])
}
pub fn runtime() -> RuntimeId {
    RuntimeId::from_bytes([10; 16])
}
pub fn root() -> RootId {
    RootId::from_bytes([11; 16])
}
pub fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, items * CATALOG_MAX_STORED_RECENCY_BYTES).unwrap()
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
        "expected atomic publication: {outcome:?}"
    );
}
pub fn publish(
    store: &HomeStore,
    state: &BerylState,
    seed: u8,
    expected: CatalogRowExpectation,
    source_revision: u64,
    activity: u64,
) {
    let sources = CatalogSourceRevisions::new(
        ProjectionRevision::new(source_revision).unwrap(),
        RecordRevision::new(1).unwrap(),
        RecordRevision::new(1).unwrap(),
        None,
    );
    let execution = CatalogExecutionSummary::new(
        runtime(),
        root(),
        "Host",
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe").unwrap(),
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Work\beryl").unwrap(),
        CatalogAvailabilitySummary::new(Availability::Available, Availability::Unknown),
    )
    .unwrap();
    let facts = CatalogFacts::new(
        CatalogResolvedTitle::history_derived(format!("History {seed}")).unwrap(),
        execution,
        CatalogArchiveSummary::Ordinary,
        UnixMillis::new(activity),
        true,
        CatalogClaimSummary::Unclaimed,
        CatalogLineageSummary::TopLevel,
    )
    .unwrap();
    committed(support::execute(
        store,
        state.catalog().publish(
            state.catalog().revision(store).unwrap(),
            PublishCatalogRow::new(thread(seed), expected, sources, facts).unwrap(),
        ),
    ));
}
pub fn seed_rows(store: &HomeStore, state: &BerylState, count: u8) -> Vec<CatalogRow> {
    for seed in 1..=count {
        publish(
            store,
            state,
            seed,
            CatalogRowExpectation::Missing,
            1,
            u64::from(seed) * 100,
        );
    }
    (1..=count)
        .map(|seed| {
            state
                .catalog()
                .row(store, thread(seed), CatalogPointReadLimit::schema_maximum())
                .unwrap()
                .unwrap()
        })
        .collect()
}
pub fn placement() -> beryl_model::WindowPlacement {
    beryl_model::WindowPlacement::new(
        beryl_model::WindowBounds::new(0, 0, 900, 700).unwrap(),
        beryl_model::WindowDisplayState::Normal,
        None,
        None,
    )
}
pub fn claim(store: &HomeStore, state: &BerylState, seed: u8) {
    let window = WindowId::from_bytes([seed; 16]);
    committed(support::execute(
        store,
        state.session().initialize_threadless(
            state.session().revision(store).unwrap(),
            InitializeThreadlessWindow::new(window, placement()),
        ),
    ));
    let bootstrap = state.session().minimal_bootstrap(store).unwrap().unwrap();
    committed(support::execute(
        store,
        state.session().replace_claim(
            state.session().revision(store).unwrap(),
            ReplaceWindowClaim::new(
                bootstrap.header().revision(),
                window,
                bootstrap.windows()[0].revision(),
                None,
                RememberedTarget::new(runtime(), root()),
                thread(seed),
            ),
        ),
    ));
}
