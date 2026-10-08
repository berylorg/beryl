use super::*;

#[test]
fn frozen_count_pages_and_exact_position_survive_live_reordering_and_snapshot_replacement() {
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
            u64::from(seed),
        );
    }
    let publisher = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let retained = store
        .retain_frozen_read(&publisher, &CommandCancellation::new())
        .unwrap();
    let mut query = owner(&store, &state);
    let opened = query
        .open(
            &store,
            retained,
            criteria(CatalogQueryScope::All, ""),
            limit(3),
            &CommandCancellation::new(),
        )
        .unwrap();
    store.release_frozen_read(&publisher).unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                publish(&store, &state, 1, 10, 11, "Changed", 999);
                publish(&store, &state, 21, 10, 11, "New live row", 1000);
            })
            .join()
            .unwrap()
    });
    assert_eq!(opened.count(), 20);
    assert_eq!(
        ids(opened.first_page()),
        vec![thread(20), thread(19), thread(18)]
    );
    let fresh_publisher = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    assert!(fresh_publisher.home_revision() > opened.home_revision());
    store.release_frozen_read(&fresh_publisher).unwrap();
    let mut all = ids(opened.first_page());
    let mut cursor = opened.first_page().next_cursor().cloned();
    while let Some(next) = cursor {
        let page = query
            .page(
                &store,
                opened.token(),
                Some(&next),
                limit(3),
                &CommandCancellation::new(),
            )
            .unwrap();
        assert_eq!(page.offset(), all.len() as u64);
        assert!(page.rows().len() <= 3);
        all.extend(ids(&page));
        cursor = page.next_cursor().cloned();
    }
    assert_eq!(all, (1..=20).rev().map(thread).collect::<Vec<_>>());
    let position = query
        .position(
            &store,
            opened.token(),
            thread(5),
            &CommandCancellation::new(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(position.index(), 15);
    let exact = query
        .page(
            &store,
            opened.token(),
            position.cursor_before(),
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(ids(&exact), vec![thread(5)]);
    assert!(
        query
            .position(
                &store,
                opened.token(),
                thread(21),
                &CommandCancellation::new()
            )
            .unwrap()
            .is_none()
    );
    query.retire(&store).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn complete_scope_normalized_substring_and_equal_activity_order_are_state_owned() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    publish(&store, &state, 3, 10, 11, "Straße", 100);
    publish(&store, &state, 1, 10, 11, "STRASSE elsewhere", 100);
    publish(&store, &state, 2, 20, 21, "strasse other root", 100);
    let mut query = owner(&store, &state);
    let all = open(
        &store,
        &mut query,
        CatalogQueryScope::All,
        "ＳＴＲＡＳＳＥ",
        16,
    );
    assert_eq!(all.criteria().search().as_str(), "strasse");
    assert_eq!(ids(all.first_page()), vec![thread(1), thread(2), thread(3)]);
    let scoped = open(
        &store,
        &mut query,
        CatalogQueryScope::Root {
            runtime_id: runtime(10),
            root_id: root(11),
        },
        "rass",
        16,
    );
    assert_eq!(scoped.count(), 2);
    assert_eq!(ids(scoped.first_page()), vec![thread(1), thread(3)]);
    assert!(
        matches!(scoped.scope_presentation(), CatalogQueryScopePresentation::Root(source) if source.root().root_id()==root(11))
    );
    let runtime_query = open(
        &store,
        &mut query,
        CatalogQueryScope::Runtime(runtime(20)),
        "",
        16,
    );
    assert_eq!(ids(runtime_query.first_page()), vec![thread(2)]);
    let path = open(&store, &mut query, CatalogQueryScope::All, "WORK\\TWO", 16);
    assert_eq!(ids(path.first_page()), vec![thread(2)]);
    query.retire(&store).unwrap();
}

#[test]
fn empty_collection_reports_zero_without_omitting_scope_facts() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    let mut query = owner(&store, &state);
    let opened = open(
        &store,
        &mut query,
        CatalogQueryScope::Runtime(runtime(10)),
        "",
        16,
    );
    assert_eq!(opened.count(), 0);
    assert!(opened.first_page().rows().is_empty());
    assert!(opened.first_page().next_cursor().is_none());
    assert!(
        matches!(opened.scope_presentation(), CatalogQueryScopePresentation::Runtime(record) if record.runtime_id()==runtime(10))
    );
    assert!(
        query
            .position(
                &store,
                opened.token(),
                thread(1),
                &CommandCancellation::new()
            )
            .unwrap()
            .is_none()
    );
    query.retire(&store).unwrap();
}
