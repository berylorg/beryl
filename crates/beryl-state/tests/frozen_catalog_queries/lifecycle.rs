use super::*;

#[test]
fn finite_collections_return_unadmitted_read_and_release_each_admitted_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let mut query = owner(&store, &state);
    let mut tokens = Vec::new();
    for _ in 0..CATALOG_QUERY_COLLECTION_LIMIT {
        tokens.push(
            open(&store, &mut query, CatalogQueryScope::All, "", 1)
                .token()
                .clone(),
        );
    }
    assert_eq!(query.pending_collections(), 32);
    assert_eq!(store.retained_frozen_read_count().unwrap(), 32);
    let rejected = store
        .capture_frozen_read(&CommandCancellation::new())
        .unwrap();
    let expected = rejected.clone();
    let failure = query
        .open(
            &store,
            rejected,
            criteria(CatalogQueryScope::All, ""),
            limit(1),
            &CommandCancellation::new(),
        )
        .unwrap_err();
    let (error, returned) = failure.into_parts();
    assert!(matches!(error, CatalogQueryError::CollectionLimit));
    assert_eq!(returned.as_ref(), Some(&expected));
    store.release_frozen_read(&returned.unwrap()).unwrap();
    query.release(&store, &tokens[0]).unwrap();
    let replacement = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    assert_ne!(replacement.token(), &tokens[0]);
    assert!(matches!(
        query.page(
            &store,
            &tokens[0],
            None,
            limit(1),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Released)
    ));
    query.retire(&store).unwrap();
    assert_eq!(query.pending_collections(), 0);
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn cancellation_releases_failed_open_but_preserves_existing_collection_until_explicit_release() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let mut query = owner(&store, &state);
    let cancel = CommandCancellation::new();
    let retained = store.capture_frozen_read(&cancel).unwrap();
    cancel.cancel();
    let failure = query
        .open(
            &store,
            retained,
            criteria(CatalogQueryScope::All, ""),
            limit(1),
            &cancel,
        )
        .unwrap_err();
    assert!(matches!(failure.error, CatalogQueryError::Cancelled));
    assert!(failure.retained().is_none());
    assert_eq!(query.pending_collections(), 0);
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
    let opened = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    assert!(matches!(
        query.page(&store, opened.token(), None, limit(1), &cancel),
        Err(CatalogQueryError::Cancelled)
    ));
    assert_eq!(query.pending_collections(), 1);
    query.release(&store, opened.token()).unwrap();
    assert_eq!(store.retained_frozen_read_count().unwrap(), 0);
}

#[test]
fn foreign_tokens_cursors_and_store_cannot_release_another_collection() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    configure(&store, &state);
    publish(&store, &state, 1, 10, 11, "one", 1);
    publish(&store, &state, 2, 10, 11, "two", 2);
    let mut query = owner(&store, &state);
    let mut other_owner = owner(&store, &state);
    let first = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    let second = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    assert!(matches!(
        other_owner.position(
            &store,
            first.token(),
            thread(1),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Foreign)
    ));
    assert!(matches!(
        query.page(
            &store,
            second.token(),
            first.first_page().next_cursor(),
            limit(1),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Foreign)
    ));
    let foreign_directory = tempfile::tempdir().unwrap();
    let (foreign_store, _) = support::open(foreign_directory.path());
    assert!(matches!(
        query.release(&foreign_store, first.token()),
        Err(CatalogQueryError::Read(ReadError::FrozenRead(
            FrozenReadAccessError::Foreign
        )))
    ));
    assert_eq!(query.pending_collections(), 2);
    assert_eq!(store.retained_frozen_read_count().unwrap(), 2);
    assert!(matches!(
        query.page(
            &store,
            first.token(),
            None,
            limit(1),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Released)
    ));
    query.release(&store, first.token()).unwrap();
    query.retire(&store).unwrap();
    other_owner.retire(&store).unwrap();
}

#[test]
fn retired_original_home_authenticates_drain_while_foreign_new_home_refuses() {
    let directory = tempfile::tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let original = store.service_reference();
    let mut query = owner(&store, &state);
    let opened = open(&store, &mut query, CatalogQueryScope::All, "", 1);
    let token = opened.token().clone();
    let cursor = opened.first_page().next_cursor().cloned();
    store.close().unwrap();
    let fresh_directory = tempfile::tempdir().unwrap();
    let (fresh, _) = support::open(fresh_directory.path());
    assert!(query.retire(&fresh).is_err());
    assert_eq!(query.pending_collections(), 1);
    query.retire(&original).unwrap();
    assert_eq!(query.pending_collections(), 0);
    assert!(matches!(
        query.page(
            &original,
            &token,
            cursor.as_ref(),
            limit(1),
            &CommandCancellation::new()
        ),
        Err(CatalogQueryError::Retired)
    ));
    drop(query);
    drop(original);
    let (reopened, _) = support::open(directory.path());
    reopened.close().unwrap();
}

#[test]
fn checked_identity_and_request_exhaustion_return_original_read_without_query_admission() {
    for identity in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        let mut query = owner(&store, &state);
        if identity {
            query.exhaust_query_identities_for_test();
        } else {
            query.exhaust_requests_for_test();
        }
        let retained = store
            .capture_frozen_read(&CommandCancellation::new())
            .unwrap();
        let exact = retained.clone();
        let error = query
            .open(
                &store,
                retained,
                criteria(CatalogQueryScope::All, ""),
                limit(1),
                &CommandCancellation::new(),
            )
            .unwrap_err();
        let (error, retained) = error.into_parts();
        assert!(if identity {
            matches!(error, CatalogQueryError::IdentityExhausted)
        } else {
            matches!(error, CatalogQueryError::RequestExhausted)
        });
        assert_eq!(retained.as_ref(), Some(&exact));
        assert_eq!(query.pending_collections(), 0);
        store.release_frozen_read(&retained.unwrap()).unwrap();
        query.retire(&store).unwrap();
    }
}
