use super::*;

#[test]
fn empty_all_scope_authenticates_runtime_root_handle_and_missing_scope_is_not_zero_success() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let foreign_directory = tempfile::tempdir().unwrap();
    let (foreign, foreign_state) = support::open(foreign_directory.path());
    let mut query = CatalogQueryOwner::new(
        state.catalog(),
        foreign_state.runtime_roots(),
        store.generation_identity().unwrap(),
    )
    .unwrap();
    let error = query
        .open(
            &store,
            store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap(),
            criteria(CatalogQueryScope::All, ""),
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error.error,
        CatalogQueryError::Read(ReadError::ForeignDomain { .. })
    ));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    assert_eq!(query.pending_collections(), 0);
    query.retire(&store).unwrap();
    let mut query = owner(&store, &state);
    let error = query
        .open(
            &store,
            store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap(),
            criteria(CatalogQueryScope::Runtime(runtime(10)), ""),
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error.error,
        CatalogQueryError::RuntimeRoot(RuntimeRootCatalogSourceError::RuntimeMissing { .. })
    ));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    query.retire(&store).unwrap();
    foreign.close().unwrap();
}

#[test]
fn stale_and_orphaned_rows_refuse_whole_query_even_outside_matching_scope() {
    for mode in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        configure(&store, &state);
        publish(&store, &state, 1, 10, 11, "matching", 1);
        publish(&store, &state, 2, 20, 21, "unrelated", 2);
        let row = state
            .catalog()
            .row(&store, thread(2), CatalogPointReadLimit::schema_maximum())
            .unwrap()
            .unwrap();
        let contribution = if mode == 0 {
            state.catalog().mark_stale(
                state.catalog().revision(&store).unwrap(),
                MarkCatalogRowStale::new(thread(2), row.revision()),
            )
        } else {
            state.catalog().remove_copy_for_test(
                state.catalog().revision(&store).unwrap(),
                row,
                mode == 1,
            )
        };
        committed(support::execute(&store, contribution));
        let mut query = owner(&store, &state);
        let error = query
            .open(
                &store,
                store
                    .capture_frozen_read(&CommandCancellation::new())
                    .unwrap(),
                criteria(CatalogQueryScope::Runtime(runtime(10)), "matching"),
                limit(1),
                &CommandCancellation::new(),
            )
            .unwrap_err();
        assert!(match error.error {
            CatalogQueryError::StaleRow { thread_id } => mode == 0 && thread_id == thread(2),
            CatalogQueryError::Catalog(CatalogReadError::Invariant(_)) => mode != 0,
            _ => false,
        });
        assert_eq!(query.pending_collections(), 0);
        assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
        query.retire(&store).unwrap();
    }
}

#[test]
fn runtime_root_presentation_stays_frozen_and_live_disagreement_requires_new_readiness() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    publish(&store, &state, 1, 10, 11, "original", 1);
    let mut query = owner(&store, &state);
    let opened = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    let original = opened.first_page().rows()[0].runtime_root().clone();
    committed(support::execute(
        &store,
        state.runtime_roots().set_root_availability(
            state.runtime_roots().revision(&store).unwrap(),
            SetRootAvailability::new(
                root(11),
                original.root().revision(),
                AvailabilitySnapshot::observed(
                    beryl_model::Availability::Available,
                    UnixMillis::new(200),
                )
                .unwrap(),
            ),
        ),
    ));
    let same = query
        .page(
            &store,
            opened.token(),
            None,
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(same.rows()[0].runtime_root(), &original);
    let failed = query
        .open(
            &store,
            store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap(),
            criteria(CatalogQueryScope::All, ""),
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(failed.error, CatalogQueryError::Structural(_)));
    assert_eq!(query.pending_collections(), 1);
    query.retire(&store).unwrap();
}

#[test]
fn maximal_valid_paths_are_reachable_with_full_schema_byte_budget_and_explicit_limits() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let expansion = "\u{fdfa}".repeat(1_092);
    let executable = format!("C:\\{}{}", "x".repeat(29_489), expansion);
    let path = format!("C:\\{}{}", "r".repeat(29_489), expansion);
    assert_eq!(executable.len(), 32 * 1024);
    assert_eq!(path.len(), 32 * 1024);
    support::create_host_runtime(&store, &state, 10, 11, &executable, &path);
    publish(&store, &state, 1, 10, 11, "large row", 1);
    let stored = state
        .catalog()
        .recency_page(
            &store,
            None,
            beryl_home_store::CursorReadLimits::new(1, CATALOG_MAX_STORED_RECENCY_BYTES).unwrap(),
        )
        .unwrap();
    assert!(stored.stored_bytes() > 190 * 1024);
    assert!(CatalogQueryPageLimit::new(0, CATALOG_QUERY_PAGE_MAX_BYTES).is_err());
    assert!(CatalogQueryPageLimit::new(17, CATALOG_QUERY_PAGE_MAX_BYTES).is_err());
    assert!(CatalogQueryPageLimit::new(1, CATALOG_QUERY_PAGE_MAX_BYTES + 1).is_err());
    let mut query = owner(&store, &state);
    let opened = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    assert_eq!(opened.count(), 1);
    assert_eq!(ids(opened.first_page()), vec![thread(1)]);
    assert!(opened.first_page().charged_bytes() <= CATALOG_QUERY_PAGE_MAX_BYTES);
    assert!(
        opened.first_page().charged_bytes() > CATALOG_MAX_STORED_RECENCY_BYTES + 2 * 132 * 1024
    );
    assert_eq!(
        opened.first_page().rows()[0]
            .runtime_root()
            .root()
            .display_path()
            .as_str(),
        path
    );
    let failure = query
        .open(
            &store,
            store
                .capture_frozen_read(&CommandCancellation::new())
                .unwrap(),
            criteria(CatalogQueryScope::All, ""),
            CatalogQueryPageLimit::new(1, 1).unwrap(),
            &CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(failure.error, CatalogQueryError::Limit));
    assert_eq!(query.pending_collections(), 1);
    query.retire(&store).unwrap();
}
