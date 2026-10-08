use super::*;

pub fn sources(
    summary: u64,
    runtime: u64,
    root: u64,
    claim: Option<u64>,
) -> CatalogSourceRevisions {
    CatalogSourceRevisions::new(
        ProjectionRevision::new(summary).unwrap(),
        RecordRevision::new(runtime).unwrap(),
        RecordRevision::new(root).unwrap(),
        claim.map(|revision| ClaimRevision::new(revision).unwrap()),
    )
}
pub fn facts(original: &CatalogRow, claim: CatalogClaimSummary, activity: u64) -> CatalogFacts {
    CatalogFacts::new(
        CatalogResolvedTitle::history_derived("Rebuilt history title").unwrap(),
        original.facts().execution().clone(),
        original.facts().archive(),
        UnixMillis::new(activity),
        original.facts().complete(),
        claim,
        original.facts().lineage(),
    )
    .unwrap()
}
pub fn rebuild(
    store: &HomeStore,
    state: &BerylState,
    expected: Option<CatalogRow>,
    sources: CatalogSourceRevisions,
    facts: CatalogFacts,
) -> CommandOutcome {
    let contribution = state.catalog().rebuild(
        state.catalog().revision(store).unwrap(),
        RebuildCatalogRow::new(thread(1), expected, sources, facts).unwrap(),
    );
    support::execute(store, contribution)
}
pub fn paired(store: &HomeStore, state: &BerylState) -> CatalogRow {
    let primary = state
        .catalog()
        .row(store, thread(1), CatalogPointReadLimit::schema_maximum())
        .unwrap()
        .unwrap();
    let recency = state
        .catalog()
        .recency_page(store, None, limits(2))
        .unwrap();
    assert_eq!(recency.rows(), &[primary.clone()]);
    primary
}
pub fn seed_high(store: &HomeStore, state: &BerylState, claimed: bool) -> CatalogRow {
    let base = seed_rows(store, state, 1).pop().unwrap();
    let claim = if claimed {
        CatalogClaimSummary::claimed(WindowId::from_bytes([1; 16]), CatalogClaimKind::Restoring)
    } else {
        CatalogClaimSummary::Unclaimed
    };
    let facts = facts(&base, claim, 100);
    committed(support::execute(
        store,
        state.catalog().publish(
            state.catalog().revision(store).unwrap(),
            PublishCatalogRow::new(
                thread(1),
                CatalogRowExpectation::Revision(base.revision()),
                sources(5, 5, 5, claimed.then_some(5)),
                facts,
            )
            .unwrap(),
        ),
    ));
    paired(store, state)
}
