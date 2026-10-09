use super::*;

#[test]
fn refinements_and_zero_thread_options_keep_original_source_after_live_changes() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    for seed in 1..=20 {
        publish(
            &store,
            &state,
            seed,
            10,
            11,
            &format!("Thread {seed}"),
            seed as u64,
        );
    }
    let mut query = owner(&store, &state);
    let opening = open(&store, &mut query, CatalogQueryScope::All, "", 2);
    assert_eq!(
        opening.first_page().rows()[0].runtime_environment_count(),
        2
    );
    publish(&store, &state, 21, 10, 11, "New live row", 1000);
    support::create_host_runtime(
        &store,
        &state,
        30,
        31,
        r"C:\New\codex.exe",
        r"C:\Work\three",
    );
    let refined = query
        .refine(
            &store,
            opening.token(),
            criteria(
                CatalogQueryScope::Root {
                    runtime_id: runtime(10),
                    root_id: root(11),
                },
                "THREAD",
            ),
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(refined.count(), 20);
    assert_eq!(refined.home_revision(), opening.home_revision());
    let tail = query
        .page_at(
            &store,
            refined.token(),
            19,
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(tail.offset(), 19);
    assert_eq!(ids(&tail), vec![thread(1)]);
    let options = query
        .runtime_page(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            0,
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(options.count(), 2);
    assert_eq!(options.rows().len(), 1);
    assert_eq!(options.rows()[0].root_count(), 1);
    assert_eq!(options.home_revision(), opening.home_revision());
    let roots = query
        .root_page(
            &store,
            opening.token(),
            runtime(20),
            &CatalogNormalizedQuery::new("OTHER\\CODEX").unwrap(),
            0,
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(roots.count(), 1);
    assert_eq!(roots.rows()[0].root().root_id(), root(21));
    assert_eq!(roots.rows()[0].thread_count(), 0);
    assert_eq!(roots.home_revision(), opening.home_revision());
    assert_eq!(
        query
            .runtime_position(
                &store,
                opening.token(),
                &CatalogNormalizedQuery::new("HOST").unwrap(),
                runtime(20),
                &CommandCancellation::new()
            )
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        query
            .root_position(
                &store,
                opening.token(),
                runtime(20),
                &CatalogNormalizedQuery::new("").unwrap(),
                root(21),
                &CommandCancellation::new()
            )
            .unwrap(),
        Some(0)
    );
    let missing = query
        .runtime_position(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            runtime(30),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(missing, None);
    query.release(&store, opening.token()).unwrap();
    assert_eq!(
        query
            .page_at(
                &store,
                refined.token(),
                19,
                limit(2),
                &CommandCancellation::new()
            )
            .unwrap()
            .rows()
            .len(),
        1
    );
    query.retire(&store).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn option_refusals_cancellation_foreign_tokens_and_refinement_saturation_preserve_source() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    publish(&store, &state, 1, 10, 11, "Original", 1);
    let mut query = owner(&store, &state);
    let opening = open(&store, &mut query, CatalogQueryScope::All, "", 2);
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert!(matches!(
        query.runtime_page(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            0,
            limit(2),
            &cancelled
        ),
        Err(CatalogQueryError::Cancelled)
    ));
    assert!(matches!(
        query
            .refine(
                &store,
                opening.token(),
                criteria(CatalogQueryScope::All, ""),
                limit(2),
                &cancelled
            )
            .unwrap_err()
            .error,
        CatalogQueryError::Cancelled
    ));
    assert_eq!(store.retained_frozen_read_count().unwrap(), 1);
    assert!(matches!(
        query.root_page(
            &store,
            opening.token(),
            runtime(10),
            &CatalogNormalizedQuery::new("").unwrap(),
            0,
            CatalogQueryPageLimit::new(1, 1).unwrap(),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Limit)
    ));
    let mut foreign = owner(&store, &state);
    assert!(matches!(
        foreign.runtime_page(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            0,
            limit(2),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Foreign)
    ));
    for _ in 1..CATALOG_QUERY_COLLECTION_LIMIT {
        query
            .refine(
                &store,
                opening.token(),
                criteria(CatalogQueryScope::All, ""),
                limit(2),
                &CommandCancellation::new(),
            )
            .unwrap();
    }
    let retained = store.retained_frozen_read_count().unwrap();
    let error = query
        .refine(
            &store,
            opening.token(),
            criteria(CatalogQueryScope::All, ""),
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap_err();
    assert!(matches!(&error.error, CatalogQueryError::CollectionLimit));
    assert!(error.retained().is_none());
    assert_eq!(store.retained_frozen_read_count().unwrap(), retained);
    query.retire(&store).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    assert!(matches!(
        query.runtime_page(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            0,
            limit(2),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Retired)
    ));
}

#[test]
fn root_recent_order_far_offsets_and_search_cover_zero_thread_registry_beyond_page_bounds() {
    use beryl_model::{AdmittedHostPath, PathFlavor, RuntimeMode, RuntimeNativePath};
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    for seed in 90..=109 {
        support::create_host_runtime(
            &store,
            &state,
            seed,
            seed,
            &format!(r"C:\Codex-{seed}\codex.exe"),
            &format!(r"C:\Runtime-{seed}"),
        );
    }
    for seed in 40..=59 {
        let path = format!(r"C:\Work\Straße-{seed}");
        let registration = RootRegistration::new(
            root(seed),
            RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, &path)
                .unwrap(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, &path).unwrap(),
            UnixMillis::new(10),
            AvailabilitySnapshot::unknown(),
        );
        committed(support::execute(
            &store,
            state.runtime_roots().add_root(
                state.runtime_roots().revision(&store).unwrap(),
                AddConfiguredRoot::new(runtime(20), registration),
            ),
        ));
        let row = state
            .runtime_roots()
            .root(&store, root(seed))
            .unwrap()
            .unwrap();
        committed(support::execute(
            &store,
            state.runtime_roots().update_root_activity(
                state.runtime_roots().revision(&store).unwrap(),
                RootActivityUpdate::new(root(seed), row.revision(), UnixMillis::new(seed as u64)),
            ),
        ));
    }
    let mut query = owner(&store, &state);
    let opening = open(&store, &mut query, CatalogQueryScope::All, "", 2);
    let search = CatalogNormalizedQuery::new("STRASSE").unwrap();
    let tail = query
        .root_page(
            &store,
            opening.token(),
            runtime(20),
            &search,
            17,
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(tail.count(), 20);
    assert_eq!(tail.offset(), 17);
    assert_eq!(
        tail.rows()
            .iter()
            .map(|row| row.root().root_id())
            .collect::<Vec<_>>(),
        vec![root(42), root(41)]
    );
    assert!(tail.rows().iter().all(|row| row.thread_count() == 0));
    assert_eq!(
        tail.rows()[0].root().last_activity_at(),
        Some(UnixMillis::new(42))
    );
    assert_eq!(
        query
            .root_position(
                &store,
                opening.token(),
                runtime(20),
                &search,
                root(40),
                &CommandCancellation::new()
            )
            .unwrap(),
        Some(19)
    );
    let empty = query
        .root_page(
            &store,
            opening.token(),
            runtime(20),
            &CatalogNormalizedQuery::new("No match").unwrap(),
            0,
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(empty.count(), 0);
    assert!(empty.rows().is_empty());
    let runtimes = query
        .runtime_page(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            0,
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(runtimes.rows()[1].root_count(), 21);
    assert_eq!(runtimes.count(), 22);
    let tail_runtime = query
        .runtime_page(
            &store,
            opening.token(),
            &CatalogNormalizedQuery::new("").unwrap(),
            21,
            limit(2),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(tail_runtime.rows().len(), 1);
    assert_eq!(tail_runtime.rows()[0].runtime().runtime_id(), runtime(109));
    assert_eq!(tail_runtime.rows()[0].root_count(), 1);
    query.retire(&store).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
}
